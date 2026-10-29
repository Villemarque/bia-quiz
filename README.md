# bia-quiz

Live, teacher-paced quiz for the French **BIA** exam, built on the
[annales database](https://github.com/Villemarque/bia-annales-database-csv).
Source: [`Villemarque/bia-quiz`](https://github.com/Villemarque/bia-quiz).

The professor projects the question; students answer with four big buttons on their
phones. Points depend on correctness and speed (0–1000, sigmoid curve). After each
question the projector shows the answer then the top 7; the session ends with a podium.

- **Backend:** Rust, axum, Diesel + SQLite, WebSockets.
- **Frontend:** plain Svelte 5 + Vite SPA, organised like SvelteKit (`src/routes/**/+page.svelte`)
  so it can migrate later without moving files. No Node in production.
- **Deploy:** one self-contained binary (SPA + migrations embedded, SQLite bundled) + one SQLite file (questions, figures, results) + systemd + Caddy.
- **Docs:** English (code and README); UI strings are French.

Reloading a game keeps your seat: students reconnect with a per-room token in
`localStorage`, and the teacher console replays the current question, reveal or
leaderboard. If a student's token is gone (private browsing, storage cleared), the
server recovers the same player when the join comes from the same client IP **and**
uses the same nickname (case-insensitive). That address only lives in memory for the
duration of the live room; it is never written to SQLite.

## Layout

```
crates/core       domain types, Diesel schema, scoring, draw, protocol
crates/server     axum app: WS room actor, static assets, /prof, /admin, auth
crates/importer   bia-import: TSV -> SQLite, baked-in chapter titles (warn + skip)
crates/server/migrations  Diesel migrations (schema source of truth)
frontend/         Svelte SPA (src/routes, src/lib, static/fonts)
data/figures/       90 .jpeg named <attachment_link>.jpeg (import source; also stored in SQLite)
deploy/           systemd unit, Caddyfile, bia.toml example, cloud-server runbook
plan.md           the design decisions and rationale
```

## Quick start

Requirements: Rust 1.86+, Node 20+, pnpm, a C compiler.

```sh
pnpm --dir frontend install
make build

./target/release/bia-import \
  --tsv annales-bia.tsv \
  --db bia.sqlite3 \
  --images data/figures

BIA_QUIZ_PROF_PASSWORD=dev BIA_QUIZ_ADMIN_PASSWORD=dev ./target/release/bia-quiz
```

Open <http://localhost:8080>: the landing page joins a game, `/teacher` starts one,
`/login` signs in, `/prof` reads stored results (any account), `/admin` imports the bank
and can end the live game (admin account only).

The release binary embeds `frontend/dist` at compile time. Figures are stored in SQLite
by `bia-import` and served from the database at runtime.

## Development

Requirements: Rust 1.86+, Node 20+, pnpm and a C compiler.

```sh
pnpm --dir frontend install
make dev
```

`make dev` starts the debug backend on `127.0.0.1:8080` and Vite on `:5173`.
Open <http://localhost:5173>; Vite proxies `/api`, `/ws`, `/figures` and `/healthz`
to the backend. The dev profile has `debug = 0`, so Rust rebuilds are faster and the
debug binary is smaller; use `debug = 1` in `Cargo.toml` if you need line tables in
backtraces.

`make run` remains the release smoke test on <http://localhost:8080>.

## Make targets

| Target | What it does |
|---|---|
| `make build` | build the SPA, then the native release binary (embeds `frontend/dist`) |
| `make artefacts` | cross-compile Linux **x86_64** and stage binaries/config in `deploy/artefacts/` |
| `make cross` / `build-artefact` | aliases for `make artefacts` |
| `make dev` | debug backend on `127.0.0.1:8080` + Vite on `127.0.0.1:5173` (open `:5173`) |
| `make run` | build the release binary, then run it on `127.0.0.1:8080` with `BIA_QUIZ_PROF_PASSWORD` and `BIA_QUIZ_ADMIN_PASSWORD` (both `dev` by default) |
| `make import` | build, download the upstream TSV + figures, then (re)import into `bia.sqlite3` |
| `make test` | `cargo test --workspace` + `svelte-check` |
| `make check` | `cargo fmt --check`, `clippy -D warnings`, `svelte-check` |
| `make uname` | print the full `uname -a` host/kernel string |
| `make doctor` | print host OS/arch vs the binary's ELF/interpreter |
| `make clean` | remove this workspace's Cargo artifacts, `frontend/dist`, `frontend/node_modules`, `.pnpm-store` (keeps the database) |
| `make distclean` | `clean` + delete `bia.sqlite3*` and `import-report.txt` (**destructive**) |

The binary path is resolved with `cargo metadata`, so **`CARGO_TARGET_DIR`**,
`build.target-dir` in `~/.cargo/config.toml`, and `CARGO_BUILD_TARGET` are honoured
(`make doctor` prints the resolved path). Override it with `make TARGET_DIR=/path ...`; if `build.target` is set in a Cargo config file (rather than `CARGO_BUILD_TARGET`), also pass `TARGET_TRIPLE=<triple>`.
Because the target directory may be shared, `make clean` removes only this workspace's
packages (`cargo clean -p ...`), not the whole cache. `deploy/artefacts/` is gitignored;
`make clean` removes generated binaries/example there but preserves a local `bia.toml`.

## Deployment

- **Blank cloud server:** [`deploy/README.md`](deploy/README.md) — Debian, DNS,
  users/firewall/Caddy/unattended upgrades/fail2ban via Ansible, build vs. architecture,
  systemd, import, backups, upgrades, troubleshooting.

## Configuration

`bia.toml` (path from `BIA_QUIZ_CONFIG`, default `./bia.toml`), every key overridable by
an environment variable:

| Key | Env | Default |
|---|---|---|
| `server.bind` | `BIA_QUIZ_BIND` | `127.0.0.1:8080` |
| `server.public_base_url` | `BIA_QUIZ_PUBLIC_BASE_URL` | `http://localhost:8080` |
| `server.max_players` | `BIA_QUIZ_MAX_PLAYERS` | `60` |
| `server.join_code_length` | `BIA_QUIZ_JOIN_CODE_LENGTH` | `3` |
| `server.session_idle_seconds` | `BIA_QUIZ_SESSION_IDLE_SECONDS` | `3600` (`0` disables; ends an inactive game session) |
| `server.source_url` | `BIA_QUIZ_SOURCE_URL` | `https://github.com/Villemarque/bia-quiz` (AGPL footer link) |
| `db.path` | `BIA_QUIZ_DB_PATH` | `bia.sqlite3` |
| `auth.session_hours` | `BIA_QUIZ_SESSION_HOURS` | `12` |
| `prof.user` | `BIA_QUIZ_PROF_USER` | `prof` |
| `prof.password` | `BIA_QUIZ_PROF_PASSWORD` | empty → prof account disabled |
| `admin.user` | `BIA_QUIZ_ADMIN_USER` | `admin` |
| `admin.password` | `BIA_QUIZ_ADMIN_PASSWORD` | empty → admin account disabled |
| `import.upstream_url` | `BIA_QUIZ_UPSTREAM_URL` | upstream GitHub archive |
| `limits.requests_per_ip_per_minute` | `BIA_QUIZ_RATE` | `240` (`0` disables) |

See `deploy/bia.toml.example`.

## Import

**From the browser (normal path):** log in at `/admin` with the admin account and run
the import. It pulls the configured `[import].upstream_url` only (no arbitrary URL, no
local folder, no upload). It downloads into a temporary directory, runs in one
transaction, reloads the figure cache, and **deactivates** questions missing from the
TSV (they are never deleted). Imports are refused while a room is live.

The CLI remains for bootstrap, CI and recovery:

```sh
bia-import --tsv annales-bia.tsv --db bia.sqlite3 --images data/figures \
           --report import-report.txt
# or: bia-import --fetch ... (download the upstream TSV + figures, then import)
```

Chapter titles are baked into the importer as a Rust constant; there is no chapters file
or CLI option.

`make import` first pulls the latest upstream archive
(`site/static/annales-bia.tsv` and `site/static/img-sujets/*.jpeg`) from GitHub, then
builds and imports. If GitHub is unreachable, it keeps the vendored copies. Pin a
version with:

```sh
make UPSTREAM=https://github.com/Villemarque/bia-annales-database-csv/archive/<ref>.tar.gz import
```

- Stores each distinct figure once in the `figures` table (`id`, `mime`, `data` BLOB) and
  serves it from SQLite; `data/figures/` is only the import source.
- Uses a **quoted TSV** parser (fields contain embedded newlines).
- Upserts on `qid`, so re-running is idempotent.
- Warns and **skips** rows it cannot use (missing figure, bad answer…). A blank
  `chapter` field imports the question with **no chapter membership**; the teacher UI
  groups those questions by subject under a `Sans chapitre` item when present.
- Accepts an optional **16th `seconds` column** (5–600) for a per-question countdown;
  without it (or empty) the session setting applies (default 30 s). The upstream TSV has
  15 columns, so a fetch overwrites a locally added column.
  Nothing is quarantined; the report lists every skipped row with its reason.
- Current file: **1 464 imported**, 0 skipped; 488 questions have no chapter, split by
  subject: Histoire (244) and Anglais (244).

## Tests

```sh
cargo test --workspace        # core unit tests + importer + one WebSocket E2E
pnpm --dir frontend check     # svelte-check
cargo clippy --workspace --all-targets
```

The E2E starts the real axum server, connects a teacher and a student over WebSockets,
plays a question, and checks the reveal and the persisted points.

## CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on pushes to `main`/`master`
and on pull requests:

- **Frontend** — pnpm install (`--frozen-lockfile`), `svelte-check`, Vite build.
- **Rust** — fmt, clippy (`-D warnings`), `cargo test --workspace` (unit + importer +
  WebSocket E2E), release build.
- Uploads the built SPA and the Linux `bia-quiz` / `bia-import` binaries as artifacts.

## Known limitations (v1)

- **Two config accounts** (`prof`, `admin`) with axum-login sessions; `/prof` and
  `/admin` (plus their APIs) are protected. The **teacher console remains open** to anyone
  with the URL (accepted risk). Sessions are in memory: a restart logs everyone out.
- **One active room.** A second start is refused, and imports are refused while a room is
  live.
- **Results kept indefinitely** with nicknames (personal data).
- No sound; figures are shown on the projector only (phones are buttons-only).
- Rate limiting is in the app: a per-IP window over HTTP requests plus a login-failure
  limiter (5 failures / 15 min per IP + username).

## Licence

Code: **AGPL-3.0** (`LICENSE`). Because the app is offered over a network, set
`server.source_url` to the deployed source. The landing page (and `/prof`, `/admin`)
shows that link plus the exact build **commit**, read from git at build time (or pinned
with `BIA_QUIZ_COMMIT=<sha>`) and linked as `<source_url>/commit/<sha>`.

The question bank (`annales-bia.tsv`) and the figures (`data/figures/`) come from
[Villemarque/bia-annales-database-csv](https://github.com/Villemarque/bia-annales-database-csv)
and are **not** covered by the AGPL grant on the code — see `NOTICE`.
