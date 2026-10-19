# Security review: `crates/server`

Scope: `src/{lib,main,config,auth,admin,api,assets,ws,room,db}.rs` and `build.rs`, plus the
importer entry point they call and the embedded frontend where it affects the findings.
This is a static review (no dynamic testing, no `cargo audit`; it is not installed here).
Axum 0.8.9, tower-sessions 0.14, axum-login 0.18, argon2 0.5.3. Date: 2026-10-18.

**Status 2026-10-19:** medium findings 2–6 are implemented. Finding 1 remains by
decision: the teacher page is public and the teacher socket is same-origin only. Lows
7–13 are still open.

## Summary

Good: Argon2 password hashing, signed `HttpOnly` + `SameSite=Lax` cookies (Secure when
`public_base_url` is https), `no-store` on teacher/admin pages, `safe_next` redirect check,
HTML escaping on the login page, Diesel parameter binding (the raw SQL contains no
user-supplied values), 64 KiB HTTP body limit, input validation on room creation, no path
handling for figures (hash-map lookup), no `unsafe`, no `{@html}` in the frontend.

The most important issue is that the **teacher WebSocket has no authentication**.
That is an accepted design decision; the socket is now restricted to the quiz origin.

| # | Severity | Finding |
|---|---|---|
| 1 | High (accepted) | `/ws?role=teacher` is unauthenticated; same-origin is now enforced |
| 7 | Low | Open redirect via `/\host` in `next` |
| 8 | Low | Argon2 verification runs on the async runtime; login username-enumeration timing |
| 9 | Low | Internal error strings returned to clients |
| 10 | Low | Join flooding / room squatting by anyone who knows the code |
| 11 | Low | Import / room race; import is not atomic with respect to room creation |
| 12 | Low | Figures and static assets are exempt from rate limiting; figures are cloned per request |
| 13 | Info | Config/operations notes (plaintext passwords, weak-password policy, token in query string) |

---

## 1. High: unauthenticated teacher WebSocket

`src/ws.rs` `ws()` upgrades every request and `handle()` dispatches on the client-supplied
`?role=teacher`. Neither `teacher_loop` nor `create_room` looks at the session; `AuthSession`
is never extracted. The HTTP routes for `/prof` and `/api/prof/*` are protected, but the
control channel is not. Any visitor (not only a student) can:

- create a room (`teacher.create`) when none exists, occupying the single room slot and
  blocking the real teacher (and blocking admin imports, which refuse while a room is live);
- when a room exists, connect with `role=teacher` and no `room` parameter: `active_room`
  matches the running room, `attach_teacher` replaces `teacher_tx`, so the legitimate teacher
  stops receiving updates and the attacker receives the roster and the correct answers at
  reveal;
- send `teacher.end`, `skip`, `pause`, `next` at will.

The end-to-end test in `tests/e2e.rs` connects as teacher with no cookie, which confirms the
behaviour. The summary of earlier design decisions says "the teacher console stays open", but
that appears to refer to the UI route; the WebSocket should still be gated. The room code does
not protect it, because `room` is optional for teachers.

Current mitigation (2026-10-19): `/ws?role=teacher` now requires an exact `Origin` match
against `server.public_base_url`; upgrades with a missing or foreign origin are rejected.
`/teacher` itself remains public by design. This stops cross-site WebSocket hijacking but
not a same-origin visitor: anyone who opens the quiz site can still create or take over the
room. If that residual risk is not acceptable, require a prof/admin session on the teacher
upgrade while leaving the page public, and bind the room to the creating user.

## 7. Low: open redirect via backslash

`safe_next` accepts `next` values that start with `/` but not `//`. `/\evil.example` passes;
browsers normalise `\` to `/` in a Location header and follow it as `//evil.example`. This is
reachable after login (`/login?next=/\evil.example`). Reject backslashes and anything that
does not parse as a relative path (also reject control characters; Axum 0.8 returns a 500
rather than panicking on invalid header values, so that part is safe).

## 8. Low: Argon2 on the runtime, enumeration timing

- `authenticate()` calls `Argon2::verify_password` synchronously inside an async function.
  Each verification costs tens of milliseconds and ~19 MiB, blocking a Tokio worker; a burst
  of logins can delay WebSocket timers for the live room. Wrap in `spawn_blocking`.
- The user lookup short-circuits when the username does not exist, so unknown usernames return
  faster than known ones (timing-based account enumeration for `prof`/`admin`). Verify against
  a dummy hash when the user is absent. Low impact (two well-known account names).
- The limiter check happens before hashing (good).
- Sessions are held in `MemoryStore` with a random signing key generated at start, so a
  restart logs everyone out (acceptable, but worth knowing; sessions are unbounded in memory
  only for authenticated users).

## 9. Low: internal errors leak to clients

`api::healthz`, `chapters`, `prof_*` and `ws::create_room` return `err.to_string()` (Diesel,
SQLite or file path text) to callers; `/healthz` and `/api/chapters` are public. Log the detail
server-side and return a generic message. `Cargo`'s `BIA_QUIZ_COMMIT` and the version are
public via `/api/meta` (by design for the AGPL source offer).

## 10. Low: joining and squatting

Students join anonymously with the six-character code displayed on the projector. Anyone who
sees the code can fill all `max_players` (default 60) slots with bots before the class
connects (each join also writes a DB row), or impersonate a name (nicknames are only
de-duplicated, not reserved). Options: teacher-controlled "lock the room", per-IP join limits,
a short join window, and shorter-lived/rotating codes. The reconnect token is 128-bit
random (`rand::thread_rng` is a CSPRNG), so identity takeover requires knowing the token. It
is passed in the WebSocket query string, which may end up in proxy/access logs; Caddy does not
log by default, but any added logging would capture it. Send it in the first message instead.

## 11. Low: import and room race

`start_import` refuses while a room is live, but `create_room` does not check whether an
import is running, and the importer opens its own SQLite connection. A room created during an
import reads a half-updated question set; the figure cache is swapped afterwards. Make the two
mutually exclusive under one lock (hold a shared state guard) and have `create_room` return an
error while `ImportJob.state` is `Downloading`/`Importing`.

The import source URL comes only from config, which is trusted, so there is no SSRF through
the API. The importer's tar extraction is a separate crate: it unpacks only whitelisted
filenames and applies a 120 s HTTP timeout, but has no maximum download or extracted size
visible in the code; add a size cap (`Read::take`) to bound a hostile or compromised
upstream.

## 12. Low: static assets and figures

`rate_limit` skips `/figures/` and `/assets/` entirely. `figure()` clones the full byte vector
for every request (`data.as_ref().clone()`), so a request flood costs memory/CPU and
bandwidth. Return the `Arc<Vec<u8>>`/`Bytes` without copying, and apply a looser limit instead
of an exemption.

## 13. Informational

- `bia.toml` stores plaintext passwords (hashed at startup); there is no minimum length or
  placeholder check, and `deploy/bia.toml.example` ships `change-me`. The Ansible `app` role
  deploys it unchanged (see `ansible_report.md` finding 1). Add a startup refusal for
  `change-me`/short passwords.
- `BIA_QUIZ_*` password environment variables are visible to root through `/proc`; fine for
  this threat model.
- Panics: several `expect("… lock")` calls on `std::sync::Mutex` (import job, figures) will
  cascade if one thread panics while holding the lock; use `lock().unwrap_or_else(|e|
  e.into_inner())` for state that is safe to reuse. A panic in the room actor ends the room.
- Dependencies were not scanned. Run `cargo audit` and `cargo deny` in CI, and pin the
  workspace with `Cargo.lock` (already committed).
- Session cookie `Secure` is derived from `public_base_url`; a misconfigured `http://` value
  silently disables it.
- The systemd unit is only partly sandboxed (see `ansible_report.md` finding 11).

## Implemented on 2026-10-19

- **2.** WebSocket: same-origin check, 4 KiB message/frame cap, per-socket message budget,
  90 s idle timeout with 30 s pings, bounded client/room queues, global connection cap.
- **3.** `same_origin()` now compares the exact configured origin (`public_base_url`),
  requires `Sec-Fetch-Site: same-origin`/`none`, and fails closed without an `Origin`.
- **4.** CSV export prefixes formula-leading nicknames; nicknames drop bidi/zero-width
  characters.
- **5.** CSP, `X-Frame-Options`, `nosniff`, `Referrer-Policy`, `Permissions-Policy`, and
  HSTS when the public URL is HTTPS.
- **6.** `X-Forwarded-For` is trusted only from a loopback peer; IPv6 is keyed by /64;
  both limiter maps prune; a per-account login lockout was added.

## Suggested order of work

1. Finding 7 (open redirect), then 8 and 9.
2. Findings 10–12, then the informational items in 13.
3. Revisit finding 1 if same-origin-only teacher control is not acceptable.
