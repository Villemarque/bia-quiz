# plan.md — `bia-quiz`

Online, live, teacher-paced quiz for the **BIA** (Brevet d'Initiation Aéronautique),
built on the existing material in this repository.

- **Question source of truth:** [`Villemarque/bia-annales-database-csv`](https://github.com/Villemarque/bia-annales-database-csv) — 1 464 exam questions, 2015–2026. The local `annales-bia.tsv` is **byte-identical** to `site/static/annales-bia.tsv` (sha256 `b0974c4a…`, 348 522 bytes).
- **Figures:** `site/static/img-sujets/` in the same repo — 90 `.jpeg` files named `<attachment_link>.jpeg`; all 90 references match, no orphans.
- **Chapter taxonomy:** the 15 upstream chapters (indices 0–14) are defined in that repo's README and baked into the importer. A blank chapter means **no chapter membership**; the teacher UI groups chapterless questions under their subject. See §10.2.
- **Visual identity:** `charte-graphique.html` — BIA · Aérien Ambassadeurs Avenir (Triple A).
- **Licence:** `LICENSE` is **AGPL‑3.0**.

Every decision below was made by the project owner through a clarifying Q&A session.
Anything that could not be derived from those answers is marked
**⚠ proposed — to confirm**, and listed in [§14 Open questions](#14-open-questions-to-confirm).

---

## 1. What we are building

A professor starts a quiz session from a browser on the classroom projector. Students
join from their phones by scanning a QR code or typing a short room code. The
**question and the four answers appear only on the teacher's screen**; each student's
phone shows **four answer buttons and nothing else**. Answers are graded on
correctness *and* speed; speed is converted to points through a **sigmoid curve**
so that a fast answer is worth far more than a slow one, and totals are hard to
track mentally. After each question the projector first shows the correct answer,
then the class leaderboard; at the end, a podium.

### Goals

1. Run a real class quiz on the existing BIA question bank, by chapter.
2. Feel like a game (big buttons, colour, shape, speed pressure) without an app install.
3. Deploy as **one static binary** behind the owner's existing **Caddy**.
4. Stay small: one active room, ~35 students, one SQLite file.

### Non-goals for v1 (backlog, see §13)

Team mode · self-paced/homework mode · practice mode with corrections · accounts ·
multi-school hosting · Postgres · question editor UI · sound.

---

## 2. Locked decisions (traceable)

| # | Area | Decision |
|---|------|----------|
| 1 | Backend | Rust, **axum** + tokio |
| 2 | Database | **Diesel + SQLite**, embedded file |
| 3 | Realtime | **WebSocket**, one socket per client |
| 4 | Frontend | **Plain Svelte + Vite SPA** (no SvelteKit), built assets **embedded in the binary** and served by axum |
| 5 | Package manager | **pnpm** |
| 6 | Repo layout | Cargo **workspace**: `server`, `importer`, `core` + `frontend/` |
| 7 | Deploy artifact | **Static binary** + **systemd** unit |
| 8 | TLS / proxy | **Caddy**, run and managed by the owner; the plan documents the contract |
| 9 | Config | **`bia.toml`**, environment variables override |
| 10 | Question import | **Separate `bia-import` command**: idempotent, and it **warns and skips** any row it cannot use instead of storing it. Nothing is quarantined |
| 11 | Images | **Included in v1**: the 190 figure-dependent questions are imported; the 90 `.jpeg` are **stored in SQLite** (`figures` table) and served at `/figures/<attachment_link>.jpeg` (§10.4) |
| 12 | Unassigned questions | **Blank chapter (488) → no `question_chapters` row**. The teacher UI groups them by `subject` under a `Sans chapitre` item. Chapter `0` is a real chapter (`1.1 Les aéronefs`, 157 records) |
| 13 | Professor auth | **None** — the teacher console is open to anyone with the URL (accepted risk, §11) |
| 14 | Results & admin auth | **Session auth via `axum-login`**: two config accounts (`prof`, `admin`), plaintext passwords hashed at startup; `/prof` and `/admin` protected, teacher console stays open (§7.7) |
| 15 | Results | Stored in SQLite, readable **only** from `/prof`, **retained indefinitely** (accepted risk) |
| 16 | Hardening | **Per-IP rate limit** + `/healthz` endpoint |
| 17 | Room state | **In memory**; results **persisted** as they happen. A restart kills the live game, never a finished session |
| 18 | Concurrent rooms | **One active room only**; a second start is refused with a clear message |
| 19 | Student screen | **Buttons only** — no question text, no answer text |
| 20 | Answer locking | Students **may change their answer until the timer ends**; the last accepted answer counts |
| 21 | Timer expiry | **Auto-reveal at 0**; the teacher then advances |
| 22 | Teacher pacing | Teacher-driven, with an optional per-question countdown |
| 23 | Speed scoring | **Sigmoid decay**, not linear (§6) |
| 24 | Leaderboard | Projector shows the **top 7 with points** |
| 25 | Student feedback | During the game: **right/wrong only** |
| 26 | Final screen | **Top-3 podium + top-7 list + each student's own result** |
| 27 | Join flow | `/` landing page → `/j/<code>`, plus **QR code + room code** on the projector |
| 28 | Late joins | **Allowed mid-question**; missed questions score 0; reconnecting keeps points via a localStorage token |
| 29 | Session setup | **Multi-chapter selection with a question count per chapter** (default 10 each; no total cap), seconds per question (default 30; a per-question TSV value wins) |
| 30 | Draw rule | Random **within each chosen chapter**, up to its count, **no repeat inside a session** |
| 31 | Question wording | `content_fixed` when present, otherwise `content_verbatim` |
| 32 | Language | Docs and comments **English**; UI strings **French** |
| 33 | Licence | **Code AGPL‑3.0** (repo `LICENSE`) + **separate content notice** for the TSV and figures; footer source link |
| 34 | Tests / CI | Unit tests + **one end-to-end WebSocket test** + GitHub Actions |
| 35 | Accessibility | Explicit criteria in §5.6 |
| 36 | Schema changes | Diesel migrations in `crates/server/migrations/`, embedded in the binary and applied at startup; forward-only; an applied migration is never edited (§7.5) |
| 37 | Answer order | `mixed_choices` is a **boolean**: when true the four choices are shuffled at question start, identically on the projector and on the phones; the stored `answer_idx` is never rewritten (§4) |
| 38 | Question data | The upstream GitHub repo is canonical; the vendored TSV is pinned by sha256, and chapters/figures come from the same repo |
| 39 | Frontend structure | Plain Svelte + Vite, but **organised exactly like SvelteKit** (`src/routes/**/+page.svelte`, `+layout.svelte`, `+error.svelte`, `+page.ts`, `$lib`, `static/`) so a later migration renames nothing (§7.6) |
| 40 | Figure storage | Every image is stored **once in SQLite** (`figures`: id, mime, data BLOB), cached in memory at startup, and served from the database; `data/figures/` is the import source only |
| 41 | Auth stack | **axum-login 0.18 + tower-sessions 0.14** (axum 0.8), in-memory session store, `HttpOnly` cookie, Argon2id password hashes (§7.7) |

---

## 3. Actors

| Actor | Device | Can do |
|-------|--------|--------|
| **Professor — live console** | Laptop on the projector | Configure a session (chapters and per-chapter counts, seconds), start/pause/skip/reveal/advance, see answers arriving and the leaderboard, end the session |
| **Student** | Phone, BYOD | Join with a code + nickname, tap one of four buttons, change it until the timer ends, see right/wrong and their final result |
| **Professor — results/admin** | Any browser + session login | Read results (`/prof`); with the admin account, import the question bank (`/admin`) |

---

## 4. Session lifecycle

```
                teacher clicks "Démarrer la partie"
   lobby  ─────────────────────────────────────────►  question (running)
     ▲                                                    │
     │                                            timer reaches 0
     │                                                    ▼
     │  teacher clicks "Question suivante"           reveal #1 (correct answer)
     │                                                    │
     │                                            2–3 s, or teacher click
     │                                                    ▼
     │                                            reveal #2 (top-7 leaderboard)
     │                                                    │
     └──────────── last question ◄────────────────────────┘
                              │
                              ▼
                        podium (end)
```

**States:** `lobby → question → reveal_answer → reveal_leaderboard → … → podium`.
Only one room exists at a time. The server is the single source of truth for time:
every client receives an absolute `ends_at` and computes the remaining time from a
clock offset measured on connect, so phone clock drift cannot affect scoring.

### Session configuration (teacher)

- Subjects: Aéronefs, Mécanique du vol, Météo, Navigation, Histoire, Anglais.
- Inside each subject: its chapters, plus a `Sans chapitre` item when that subject has
  chapterless questions. Each selected item gets its own count (default **10**); there
  is no total cap.
- Seconds per question: session fallback **30**; a question's optional `seconds` value wins.
- Start → a room code and QR code appear on the projector.

### Draw rule

Random selection inside each selected subject/chapter pool (or subject/no-chapter
pool), excluding every question already used **in this session**. If a pool has fewer
usable questions than requested, the session draws what exists and warns the teacher
before starting.

A question belongs to **every** chapter listed in its `chapter` field: the upstream
format allows a comma-separated list, so a question tagged `3,4` can be drawn from
either chapter. The importer stores one membership row per chapter (§7.4).

### Answer order (shuffling)

`mixed_choices` is a **per-question boolean** in the TSV, and true means **the four
choices are shuffled**. The shuffle is done **once, server-side, when the question
starts**, and the resulting order is sent identically to the projector and to every
student's buttons — otherwise the teacher's screen and the phones would disagree about
which shape is which choice.

- `answer_idx` keeps the **original** index from the TSV. The server maps
  *displayed position ↔ original index* for grading and for the reveal; the TSV is
  never rewritten.
- Blank or `FALSE` means **no shuffle** (the TSV order is displayed as-is).
- The reveal highlights the **displayed position** of the correct choice, and the
  per-choice counts follow the displayed order.
- The shuffle used by each question is recorded in the session (`display_order`) so
  the results view (`/prof`) can label positions later.

### Join, late join, reconnect

- Landing page `/` → **Rejoindre** (`/j/<code>` with the code pre-filled from the QR)
  or **Espace professeur**.
- A student types a nickname once. The server issues a **player token**; the token is
  stored in `localStorage` so a refresh, a lock screen or a Wi-Fi drop rejoins the
  **same player with the same points**.
- Duplicate nicknames get an automatic suffix (`Max`, `Max (2)`).
- Joining mid-question is allowed: the late student can answer the current question
  if the window is still open; missed questions score 0.

### Teacher controls (v1)

`Démarrer la question` · `Révéler maintenant` (force reveal before 0) · `Question suivante` ·
`Pause` / `Reprendre` · `Passer` (skip a question, excluded from results).

While paused the countdown is frozen. Students **may still answer**: the answer window
stays open and speed points are computed from the remaining time at the instant of the
pause (the paused duration is not counted).

Not in v1: removing a player, going back, editing the stored question list.

---

## 5. UX

### 5.1 Teacher / projector screen

**Lobby** — room code in very large type, QR code, join instructions, live list of
joined nicknames, chapter/count/timer summary, `Démarrer la question`.

**Question** — the question text (large), the four choices **in the shuffled display
order** (§4), rendered with the same shape + letter + colour as the student's buttons,
the countdown ring/bar, the figure if the question has one (scaled to fit, §10.4), and
the number of answers received (e.g. `12 / 28 réponses`). No distribution is shown yet.
Figures appear **only on the projector** — phones stay buttons-only (§5.2).

**Reveal #1 — correct answer** (server-triggered at 0): the correct choice
highlighted, the others dimmed; the figure stays visible so the class can see what the
question was about. ⚠ proposed — confirm: also show per-choice counts
(a small bar under each choice).

**Reveal #2 — leaderboard** (⚠ proposed: appears 2–3 s after reveal #1, or on a teacher
click): **top 7** with nickname + points, animated in. The teacher then clicks
`Question suivante`.

**Podium** — top 3 animated, then the top-7 list; each student sees their final rank
and points on their phone.

Buttons: `Révéler maintenant`, `Pause`, `Passer`, `Question suivante`, `Terminer`.

### 5.2 Student phone

Deliberately minimal — the phone is a **buzzer**, not a screen to read.

| State | What is shown |
|-------|---------------|
| Join | Code (pre-filled), nickname field, `Rejoindre` |
| Lobby | "En attente du professeur…", nickname |
| Question | Four large buttons — **shape + letter + symbol + colour** — and a timer bar, plus `Réponse envoyée : ◯` once answered |
| Answered | Buttons dim; tapping another one **replaces** the answer while the timer runs |
| Reveal | **`Bonne réponse` / `Mauvaise réponse`** only — no points, no rank, no running total |
| End | Own rank + own points + a short thank-you |

### 5.3 Answer buttons

Four distinguishable shapes, each with a letter and a symbol, so colour is never the
only channel:

⚠ proposed — confirm this shape/letter/symbol set:

| Choice | Shape | Letter | Symbol |
|--------|-------|--------|--------|
| A | Triangle | A | ▲ |
| B | Diamond | B | ◆ |
| C | Circle | C | ● |
| D | Square | D | ■ |

Colours derived from the charte palette (⚠ proposed — confirm exact mapping against
the charte's "Combinaisons" section before implementing).

Shuffling only changes which text sits at which position: each position keeps its
shape, letter and colour, so the projector and the phones always agree on what a
student's button means.

### 5.4 Professor results (`/prof`, session auth)

Requires a login (`prof` or `admin` account, §7.7). Read-only in v1: list of sessions (date, chapters, count, players), detail per session
(per-question distribution, per-student answers and totals), CSV export of a session.
This is the only surface where stored results are visible. No question editing
(questions come from the TSV).

### 5.5 Design system

The UI **must be in sync with `charte-graphique.html`**:

- Typeface **Source Sans Pro**, self-hosted (the app must not depend on Google Fonts or
  any third-party CDN), with a system-font fallback.
- Tokens from the charte: navy `#000a33` / `#071037`, accent coral `#e97770`
  (hover `#f98780`, active `#d96760`), ink `#6e737a`, surfaces `#fafafa` / `#ffffff`.
- Reuse the charte's components: onglet de section, bouton d'action (CTA), élément de
  liste à icône, barre de navigation, lien.
- The teacher screen is a full-screen projector view; the student view is a mobile-first
  full-screen view.

### 5.6 Accessibility criteria (acceptance criteria)

- Every answer button is identifiable **without colour**: shape + letter + symbol +
  `aria-label` in French.
- Minimum tap target **44 × 44 px**; no hover-only affordances.
- Visible focus rings; the **teacher console is fully keyboard-operable**
  (start / next / pause / skip).
- `prefers-reduced-motion` disables transitions and the podium animation.
- Contrast checked against the charte palette for text, buttons and the reveal states
  (target: WCAG AA 4.5:1 for text, 3:1 for large text and UI boundaries).
- French typography: real accents, non-breaking space before `: ; ! ?`, `« »` quotes.
- The timer is also conveyed by a number, not only by a shrinking bar.
- Figures carry a French `alt` text. The bank has no descriptions yet, so use
  `alt="Figure associée à la question"` and treat proper descriptions as an editorial gap.
- No sound is required to follow the game (there is no sound in v1).

---

## 6. Scoring

Correctness gate first: **wrong answer or no answer ⇒ 0 points. No negative points.**
An answer received after the deadline is rejected (the server already auto-revealed).

For a correct answer, points are **not** linear in the time left. Speed is mapped
through a **sigmoid** so a decisive student is rewarded hard and a trailing student
drops off a cliff — which also makes totals hard to track mentally:

```
t   = remaining_seconds / question_seconds     in [0, 1]
s(t) = 1 / (1 + exp(-k * (t - m)))             logistic
points = round(1000 * (s(t) - s(0)) / (s(1) - s(0)))
```

`s(0)` / `s(1)` normalise the curve so a last-instant correct answer scores 0 and an
instant answer scores 1000, while the middle is where the curve is steepest.

⚠ proposed — to confirm:
- steepness `k` and midpoint `m` (a starting point for discussion: `k = 8`, `m = 0.5`,
  which gives roughly `0 / 105 / 500 / 895 / 1000` at `t = 0 / 0.25 / 0.5 / 0.75 / 1`:
  a fast student is rewarded hard, a slow one drops off, and the middle is steep).
- rounding to the nearest integer.
- whether a tiny floor (e.g. "a correct answer is never worth 0") is wanted.

Speed is measured **server-side** from the moment the question opens, using the
**last accepted** answer for a student who changed their mind.

---

## 7. Architecture

```
        phone (student)                    laptop (projector)
        ┌───────────────┐                  ┌────────────────────┐
        │ Svelte SPA    │                  │ Svelte SPA         │
        │ /j/<code>     │                  │ /teacher, /prof    │
        └──────┬────────┘                  └─────────┬──────────┘
               │  WebSocket /ws (role, room, token)  │
               └──────────────┬──────────────────────┘
                              ▼
                  ┌────────────────────────────┐
                  │  bia-quiz (Rust, axum)     │
                  │  ├ static: embedded SPA    │
                  │  ├ REST: /healthz, /prof   │
                  │  ├ WS:   /ws               │
                  │  ├ rooms (in memory)       │
                  │  └ scorer (core crate)     │
                  └───────────┬────────────────┘
                              ▼
                     bia.sqlite3  (Diesel)
                              ▲
                  bia-import (TSV → DB, one-shot)
```

### 7.1 Components

| Component | Role |
|-----------|------|
| `crates/core` | Domain types, the **scorer**, the room state machine, draw logic, protocol message types. Pure, heavily unit-tested. |
| `crates/server` | axum app: embeds `frontend/dist` and the Diesel migrations; serves the SPA, `/figures/*` (from SQLite), the WebSocket, `/healthz`, `/prof`, `/admin`, the rate limiter, config loading. |
| `crates/server/src/auth.rs` | axum-login backend (config accounts, Argon2id), role checks, login/logout handlers, login rate limit (§7.7). |
| `crates/importer` | `bia-import` binary: reads the TSV, validates, **warns and skips unusable rows**, upserts the rest into SQLite, prints a report. Bakes the 15 chapter titles into the binary; no chapters file or flag (§10.2). Never runs inside the server. |
| `frontend/` | Svelte + Vite + pnpm, tree shaped like SvelteKit (§7.6): `src/routes/`, `src/lib/`, `static/`, plus the shared charte CSS tokens. |
| `data/figures/` | The 90 upstream `.jpeg` figures, vendored byte-for-byte; `bia-import` stores them in SQLite (§10.4). |
| `deploy/` | systemd unit, sample `Caddyfile`, `bia.toml` example, install/upgrade notes. |

### 7.2 Frontend ↔ backend

The SPA is served by axum as the fallback route; there is **no Node process in
production**. During development, `vite dev` proxies API/WS calls to the axum process
(`pnpm dev` + `cargo run`). A release build runs `pnpm build` before `cargo build
--release`, and the `frontend/dist` folder is compiled into the binary.
⚠ proposed — a `justfile`/`Makefile` with `make dev` / `make build` / `make import` targets.

### 7.3 WebSocket protocol (JSON, one socket per client)

Connect: `wss://<host>/ws?role=teacher|student&room=<CODE>&token=<player-token?>`

Server → client:

| Message | Payload |
|---------|---------|
| `room.state` | room code, phase, player count, joined nicknames |
| `question.start` | `qid`, question text, `figure_url` (nullable), the four choices **in display order** (text, shape, letter), `display_order` (mapping to the TSV indices), `ends_at` (server epoch ms), question index |
| `answers.count` | number of answers received so far |
| `question.reveal` | correct **displayed** index, per-choice counts ⚠, correct choice text |
| `leaderboard` | top 7 `{nickname, points}` after each question |
| `session.end` | podium (top 3 + top 7) and the caller's own rank/points |
| `player.removed` | (reserved, not used in v1) |
| `error` | code + French message |

Client → server:

| Message | From |
|---------|------|
| `teacher.configure` / `teacher.start` | teacher |
| `teacher.start_question`, `teacher.force_reveal`, `teacher.next`, `teacher.pause`, `teacher.resume`, `teacher.skip`, `teacher.end` | teacher |
| `student.join` `{nickname}` | student |
| `student.answer` `{choice: 0..3}` | student — `choice` is the **displayed** position; the server maps it back to the original index. Idempotent: replaces the previous answer |
| `ping` / `pong` | both (clock offset + liveness) |

The server never trusts client time or client scoring.

### 7.4 Database schema (Diesel + SQLite)

Three artefacts, in order of authority:

1. `crates/server/migrations/<version>_initial_schema/up.sql` — the **source of truth**.
2. `crates/server/src/schema.rs` — **generated** from the live database with
   `diesel print-schema` (configured in `diesel.toml`); never hand-edited (§7.5).
3. The Rust structs (`Queryable` / `Selectable` / `Insertable`) — compiled against
   `schema.rs`.

#### `schema.rs` (generated)

```rust
// @generated automatically by Diesel CLI.

diesel::table! {
    answers (id) {
        id -> Integer,
        session_question_id -> Integer,
        player_id -> Integer,
        choice_idx -> Integer,
        is_correct -> Bool,
        points -> Integer,
        answered_at -> BigInt,
        ms_elapsed -> Integer,
    }
}

diesel::table! {
    chapters (id) {
        id -> Integer,
        number -> Integer,
        title -> Text,
    }
}

diesel::table! {
    figures (id) {
        id -> Text,
        mime -> Text,
        data -> Binary,
    }
}

diesel::table! {
    players (id) {
        id -> Integer,
        session_id -> Integer,
        token -> Text,
        nickname -> Text,
        joined_at -> BigInt,
    }
}

diesel::table! {
    question_chapters (question_qid, chapter_id) {
        question_qid -> Text,
        chapter_id -> Integer,
    }
}

diesel::table! {
    questions (qid) {
        qid -> Text,
        year -> Integer,
        subject -> Integer,
        text -> Text,
        used_fixed -> Bool,
        choice_a -> Text,
        choice_b -> Text,
        choice_c -> Text,
        choice_d -> Text,
        answer_idx -> Integer,
        mixed_choices -> Bool,
        attachment_link -> Nullable<Text>,
        active -> Bool,
        seconds -> Nullable<Integer>,
    }
}

diesel::table! {
    session_chapters (session_id, chapter_id) {
        session_id -> Integer,
        chapter_id -> Integer,
    }
}

diesel::table! {
    session_questions (id) {
        id -> Integer,
        session_id -> Integer,
        position -> Integer,
        qid -> Text,
        display_order -> Text,
    }
}

diesel::table! {
    sessions (id) {
        id -> Integer,
        code -> Text,
        started_at -> BigInt,
        ended_at -> Nullable<BigInt>,
        question_count -> Integer,
        seconds_per_question -> Integer,
    }
}

diesel::joinable!(answers -> players (player_id));
diesel::joinable!(answers -> session_questions (session_question_id));
diesel::joinable!(players -> sessions (session_id));
diesel::joinable!(question_chapters -> chapters (chapter_id));
diesel::joinable!(question_chapters -> questions (question_qid));
diesel::joinable!(session_chapters -> chapters (chapter_id));
diesel::joinable!(session_chapters -> sessions (session_id));
diesel::joinable!(session_questions -> questions (qid));
diesel::joinable!(session_questions -> sessions (session_id));

diesel::allow_tables_to_appear_in_same_query!(
    answers,
    chapters,
    figures,
    players,
    question_chapters,
    questions,
    session_chapters,
    session_questions,
    sessions,
);
```

#### `up.sql` (the migration)

```sql
CREATE TABLE chapters (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    number  INTEGER NOT NULL UNIQUE,
    title   TEXT    NOT NULL
);

CREATE TABLE figures (
    id   TEXT NOT NULL PRIMARY KEY,
    mime TEXT NOT NULL,
    data BLOB NOT NULL
);

CREATE TABLE questions (
    qid             TEXT    NOT NULL PRIMARY KEY,
    year            INTEGER NOT NULL,
    subject         INTEGER NOT NULL CHECK (subject BETWEEN 0 AND 5),
    text            TEXT    NOT NULL,
    used_fixed      BOOLEAN NOT NULL DEFAULT 0,
    choice_a        TEXT    NOT NULL,
    choice_b        TEXT    NOT NULL,
    choice_c        TEXT    NOT NULL,
    choice_d        TEXT    NOT NULL,
    answer_idx      INTEGER NOT NULL CHECK (answer_idx BETWEEN 0 AND 3),
    mixed_choices   BOOLEAN NOT NULL DEFAULT 0,
    attachment_link TEXT,
    active          BOOLEAN NOT NULL DEFAULT 1,
    seconds         INTEGER
);

CREATE TABLE question_chapters (
    question_qid TEXT    NOT NULL REFERENCES questions (qid) ON DELETE CASCADE,
    chapter_id   INTEGER NOT NULL REFERENCES chapters  (id)  ON DELETE CASCADE,
    PRIMARY KEY (question_qid, chapter_id)
);

CREATE INDEX question_chapters_chapter_id_idx ON question_chapters (chapter_id);

CREATE TABLE sessions (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    code                 TEXT    NOT NULL,
    started_at           BIGINT  NOT NULL,
    ended_at             BIGINT,
    question_count       INTEGER NOT NULL,
    seconds_per_question INTEGER NOT NULL
);

CREATE INDEX sessions_started_at_idx ON sessions (started_at);

CREATE TABLE session_chapters (
    session_id INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters (id),
    PRIMARY KEY (session_id, chapter_id)
);

CREATE TABLE session_questions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    qid           TEXT    NOT NULL REFERENCES questions (qid),
    display_order TEXT    NOT NULL,
    UNIQUE (session_id, position)
);

CREATE TABLE players (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    token      TEXT    NOT NULL UNIQUE,
    nickname   TEXT    NOT NULL,
    joined_at  BIGINT  NOT NULL
);

CREATE INDEX players_session_id_idx ON players (session_id);

CREATE TABLE answers (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    session_question_id INTEGER NOT NULL REFERENCES session_questions (id) ON DELETE CASCADE,
    player_id           INTEGER NOT NULL REFERENCES players (id) ON DELETE CASCADE,
    choice_idx          INTEGER NOT NULL CHECK (choice_idx BETWEEN 0 AND 3),
    is_correct          BOOLEAN NOT NULL,
    points              INTEGER NOT NULL CHECK (points BETWEEN 0 AND 1000),
    answered_at         BIGINT  NOT NULL,
    ms_elapsed          INTEGER NOT NULL,
    UNIQUE (session_question_id, player_id)
);

CREATE INDEX answers_player_id_idx ON answers (player_id);
```

`down.sql` drops in reverse dependency order (`answers`, `players`,
`session_questions`, `session_chapters`, `sessions`, `question_chapters`,
`questions`, `chapters`) and is exercised by the migration round-trip test (§12).

Notes:

- `table!` **cannot express** `UNIQUE`, `CHECK`, `DEFAULT`, indexes or FK actions, so
  those live only in `up.sql`. That is exactly why the migration is the source of truth
  and `schema.rs` is regenerated.
- **Timestamps are `BIGINT` epoch milliseconds**, written by the server — not SQLite
  `datetime` — so the speed score and the reveal never depend on SQLite date parsing or
  on any client clock.
- `answers` holds **one row per player per question**, upserted on
  `(session_question_id, player_id)`; a student who changes their answer updates the row,
  and the last accepted answer wins (§2 #20).
- `answers.choice_idx` stores the **original** TSV index; the shuffle actually shown is
  `session_questions.display_order` (e.g. `2,0,3,1`), so stats are shuffle-independent.
- `players.token` is the rejoin credential stored in the student's `localStorage` (§4);
  unique by construction.
- `sessions.ended_at IS NULL` means the session was interrupted (crash/restart); a
  finished session always has it set. The live room itself is in memory (§2 #17).
- `session_chapters` and `question_chapters` are join tables because both a session and
  a question can involve several chapters (§8 rule 4).
- `attachment_link` is the figure id: the import source is `data/figures/<id>.jpeg`; the
  bytes live in `figures` and are served at `/figures/<id>.jpeg` (§10.4).
- `figures` stores each image once (`id`, `mime`, `data BLOB`); the server loads the table
  into memory at startup and serves from there. Pruning happens at import time.
- `seconds` is the optional per-question countdown (5–600 s); when NULL the room uses the
  session fallback (`sessions.seconds_per_question`, default 30).
- SQLite does **not** enforce foreign keys by default: the server runs
  `PRAGMA foreign_keys = ON` (plus `journal_mode = WAL` and a `busy_timeout`) on every
  pooled connection.
- `used_fixed` records whether `content_fixed` supplied the wording (§8 rule 6);
  `subject` 0–5 groups the teacher selector into subjects and scopes chapterless pools.

### 7.5 Schema evolution

The schema will change during the project's life, and the app must upgrade an existing
`bia.sqlite3` without manual steps. Diesel's migration system is the whole mechanism —
there is no "schema version" constant to bump by hand.

**Layout.** Migrations live in `crates/server/migrations/` and are named
`{UTC timestamp}_{name}` (Diesel uses the prefix as the version and runs them in that
order):

```
migrations/20261017000000_initial_schema/{up.sql,down.sql}
```

**Tracking.** Diesel records applied versions in `__diesel_schema_migrations`
(`version`, `run_on`). Each migration runs **once**, so `up.sql` does not need to be
idempotent. Every migration gets its own transaction by default: a failure rolls back
and **startup aborts**, so the server never serves against a half-migrated DB. A
migration that cannot run in a transaction (see SQLite below) declares it next to the
migration:

```toml
# migrations/<version>_<name>/metadata.toml
run_in_transaction = false
```

**Startup.** The migrations are compiled into the binary and applied before the HTTP
listener binds:

```rust
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

let mut conn = establish_connection();
conn.run_pending_migrations(MIGRATIONS).expect("migration failed");
```

This is what makes the §9.1 "replace the binary and restart" upgrade story work: on
`systemctl restart`, pending migrations apply, then the server starts.

**`schema.rs`.** Diesel generates `schema.rs` from the live database (`diesel.toml`
points at it) and updates it whenever a migration is run or reverted. The code compiles
against that file, so the development loop is: write `up.sql` → `diesel migration run`
→ `schema.rs` regenerates → the compiler lists every query the change broke. CI runs
the migrations against a temporary SQLite and fails if the committed `schema.rs` is
stale (`diesel migration run --locked-schema`).

**Rules.**

1. **Never edit a migration that has already been applied.** Diesel stores only the
   version, not a checksum: editing it is a silent no-op on existing databases, which
   then stop matching the file. Add a new migration instead (in development,
   `diesel migration redo` or delete the DB).
2. `down.sql` is **handwritten** and is not exercised by a normal upgrade. Each
   migration must ship a real `down.sql`; the test suite applies and reverts every
   migration once (`migration redo`) against a temp DB.
3. Data backfills belong **inside** the migration (e.g. adding a `NOT NULL` column means
   adding it nullable or with a default, backfilling, then tightening it).
4. **Back up `bia.sqlite3` before any release that contains a migration.** There is no
   automatic downgrade: rolling back a release means restoring the backup and the
   previous binary.
5. The importer is idempotent (upsert on `qid`, §8), so re-running `bia-import` after a
   schema change is always safe.

**SQLite caveats (this project).**

- SQLite's `ALTER TABLE` is minimal. Adding a column works (a `NOT NULL` column needs a
  default); `DROP COLUMN` requires SQLite ≥ 3.35; changing a column's type or a
  constraint requires the standard **create new table → copy → drop → rename** sequence
  written by hand in `up.sql`.
- `PRAGMA foreign_keys` cannot be changed inside a transaction, which is exactly what a
  table rebuild needs — so rebuild migrations must set `run_in_transaction = false`
  and do their own backup ordering.
- Such a migration is the one case where "one migration = one transaction" does not
  hold; the plan accepts that, and the migration must leave the DB consistent if it
  fails (SQLite table-rebuild steps are written to be re-runnable after a manual
  cleanup, and the backup from rule 4 is the safety net).

### 7.6 Frontend structure (SvelteKit-shaped, without SvelteKit)

The app is a plain Svelte + Vite SPA (§2 #4), but the frontend is laid out **exactly as
SvelteKit would lay it out**. Only `src/router/` and the tiny `src/lib/shim/` are
non-Kit. The point is that changing your mind becomes a configuration change, not a
rewrite.

```
frontend/
├── index.html                  # becomes src/app.html under Kit
├── svelte.config.js
├── vite.config.ts              # $lib alias, publicDir: 'static'
├── static/                     # Kit convention: fonts, favicon, logo
└── src/
    ├── app.css                 # charte tokens (navy, coral, Source Sans Pro)
    ├── main.ts                 # mounts <Router />
    ├── lib/                    # == $lib
    │   ├── components/         # shared, route-agnostic
    │   ├── stores/             # room.svelte.ts, player.svelte.ts
    │   ├── api.ts              # REST helpers
    │   ├── ws.ts               # WebSocket client + reconnect
    │   └── shim/navigation.ts  # goto/afterNavigate, same API as $app/navigation
    ├── router/                 # the only Kit-specific code to delete later
    │   ├── Router.svelte
    │   └── match.ts
    └── routes/
        ├── +layout.svelte      # global shell, renders {children}
        ├── +layout.ts          # export const ssr = false; prerender = false
        ├── +error.svelte       # 404 + runtime errors
        ├── +page.svelte        # landing (/)
        ├── j/[code]/
        │   ├── +page.svelte
        │   └── +page.ts        # load({ params }) -> { code }
        ├── teacher/
        │   └── +page.svelte
        └── prof/
            └── +page.svelte
```

Rules:

1. **Filenames are SvelteKit's.** `+page.svelte`, `+layout.svelte`, `+error.svelte`,
   `+page.ts`. A migration renames nothing.
2. **Routing is derived from the tree**, never from a hand-maintained table:
   `Router.svelte` builds the map with `import.meta.glob('/src/routes/**/+page.svelte')`
   and turns `[code]` folders into params. Adding a screen = adding a folder.
3. **`+page.ts` is the only place a route reads inputs** (`params`, `url`, `fetch`); its
   `load()` returns the `data` prop that `+page.svelte` receives, exactly like Kit. Route
   components never read `location` directly.
4. **`+layout.svelte` renders `{children}`** (Svelte 5 snippet), so nesting behaves like
   Kit. The router composes layouts from the root down to the matched route (one level
   today).
5. **`+error.svelte`** receives `{ status, error }`; the router renders it for unknown
   paths and for exceptions thrown in `load()` or a component.
6. **`$lib` resolves to `src/lib`** (Vite alias). No `../../` imports across routes.
7. **`static/` is the asset root** (`publicDir: 'static'`), matching Kit; the self-hosted
   Source Sans Pro, favicon and logo live there.
8. **No browser global at module scope.** `localStorage`, `window`, `document` only inside
   `onMount`/`$effect` or behind `lib/env.ts`; the same components can then run under SSR
   later without edits.
9. **Navigation only through `lib/shim/navigation.ts`** (`goto`, `afterNavigate`), whose
   call sites and signatures match `$app/navigation`.
10. **State lives in `src/lib/stores/*.svelte.ts`**, not in route components, so a future
    `+page.ts` can select data without touching the UI.
11. **Route-private components stay in the route folder** (e.g.
    `routes/teacher/Scoreboard.svelte`); shared ones move to `lib/components/`.
12. **`+layout.ts` already declares `ssr = false` and `prerender = false`** — inert today,
    correct the day Kit takes over, and it documents that the app is client-only.

Deliberately **not** emulated because they are not used: SSR, `+page.server.ts`, form
actions, hooks, and the rest of the `$app/*` surface.

**Migration recipe if you change your mind** (~1–2 hours, no component rewrites):

1. add `@sveltejs/kit` and `adapter-static` with `fallback: 'index.html'`;
2. rename `index.html` → `src/app.html` and add `%sveltekit.head%` / `%sveltekit.body%`;
3. delete `src/router/` and `src/lib/shim/`, and import `goto` from `$app/navigation`;
4. keep every `+page.svelte`, `+layout.svelte`, `+error.svelte` and `+page.ts` in place;
5. point `rust-embed` at `build/` (adapter-static output) instead of `dist/`.

Add one line to `frontend/README.md`: *"This tree follows SvelteKit's conventions on
purpose; `src/router/` is the only non-Kit piece."*

### 7.7 Authentication (axum-login)

Results and admin use **session authentication**, not HTTP Basic. Stack:
`axum-login 0.18` over `tower-sessions 0.14` (both target axum 0.8).

**Accounts.** Two service accounts, declared in config and loaded at startup:

| account | role | can access |
|---|---|---|
| `prof` | prof | `/prof`, `/api/prof/*` |
| `admin` | admin | `/prof`, `/api/prof/*`, `/admin`, `/api/admin/*` |

Passwords are written **plaintext in `bia.toml`** and hashed with Argon2id **at startup**
(the file must be `0640 root:bia-quiz`). The in-memory hash is the `session_auth_hash`, so
changing a password requires a restart and invalidates existing sessions. Environment
overrides (`BIA_QUIZ_PROF_PASSWORD`, `BIA_QUIZ_ADMIN_PASSWORD`) follow the same path.

**Backend.** An in-memory `AuthnBackend` (`Arc<Vec<User>>` built from config) implements
`authenticate` (verify Argon2) and `get_user` (lookup by id). No `users` table: the two
accounts are deployment configuration. `admin` is a **superset of `prof`** (it can read
results too). The teacher console (`/teacher`) stays open (§2 #13).

**Sessions.** `tower-sessions` `MemoryStore`, cookie `bia_session`: `HttpOnly`,
`SameSite=Lax`, `Path=/`, `Secure` when `public_base_url` starts with `https`, max age
`auth.session_hours` (default 12 h). Sessions are lost on restart; both accounts simply
log in again. A SQLite session store is a backlog item if persistence is wanted.

**Routes.**

| route | protection |
|---|---|
| `/`, `/j/:code`, `/teacher`, `/ws`, `/figures/*`, `/api/chapters`, `/healthz` | public |
| `/login`, `POST /login`, `POST /logout` | public (rate-limited) |
| `/prof` | `login_required!` → redirect to `/login?next=…` (any role) |
| `/admin` | `login_required!` **and** role `admin`, else 403 |
| `/api/prof/*` | `AuthSession`, any role → 401 JSON if absent |
| `/api/admin/*` | `AuthSession` + role `admin` → 401/403 JSON |

**Login flow.** `GET /login` serves a small server-rendered French form (charte). `POST
/login` verifies credentials, calls `auth_session.login(&user)`, then redirects to a
validated local `next` (default `/prof`) or re-renders the form with an error. `POST
/logout` calls `auth_session.logout()` and redirects to `/login`.

**CSRF.** The cookie is `SameSite=Lax`; every mutating handler also checks that
`Origin` / `Sec-Fetch-Site` is same-origin. No separate token in v1.

**Brute force.** The global per-IP window applies, plus a login-specific limiter of 5
failures per (IP, username) per 15 minutes, with a generic error message.


---

## 8. Import contract (`bia-import`)

Input: `--tsv annales-bia.tsv`, `--db bia.sqlite3`, `--images data/figures` (required in
v1). Chapter titles are baked into the importer; there is no chapters configuration.
Idempotent: **upsert on `qid`**, so re-running after a
TSV fix is safe.

Rules (in order):

| # | Rule |
|---|------|
| 1 | Parse with a **quoted TSV reader** (RFC 4180 style), never by splitting lines: several fields contain embedded newlines and `"` quotes — the file has 1 477 physical lines for **1 464 records**. Accept 15 columns, or 16 with a trailing `seconds` column. Blank lines are dropped |
| 2 | Duplicate `qid` → the first row wins; the others are **warned and skipped** (`duplicate_qid`). None today: all 1 464 `qid` are unique |
| 3 | Defensive guard — not triggered by the current file, kept so a future edit cannot corrupt the DB silently: missing `qid`, `answer` outside `0..=3`, or an empty choice → **warn and skip** (`bad_row`), never dropped without a warning |
| 4 | `chapter` blank → **no `question_chapters` row** (chapterless). Otherwise split on commas, trim and dedupe; each value must be an integer **0–14**, and one `question_chapters` row is written per chapter |
| 5 | `attachment_link` non-empty → the importer **requires** `<images>/<attachment_link>.jpeg`; a missing file is a **warn and skip** (never import a question whose figure is absent). All 190 such rows resolve today (§10.4) |
| 6 | `text` = `content_fixed` when non-empty, else `content_verbatim`; `used_fixed` (bool) records which was used |
| 7 | `mixed_choices` is parsed to a real **boolean**: `TRUE` → true (925), `FALSE` or blank → false (13 + 526). True means the four choices are **shuffled at display time** (§4); the stored `answer_idx` is never rewritten |
| 8 | ⚠ optional safeguard to confirm: also **warn and skip** rows whose *displayed* text refers to a figure (`ci-contre`, `photographie`, `schéma`, `figure`, `graphique`) even without an `attachment_link` — v1 then ships zero broken questions |
| 9 | Every chapter number that survives rule 4 must exist in the baked-in chapter table (0–14); a missing title is a hard error, so a mis-typed chapter cannot silently vanish |
| 10 | Optional 16th column `seconds` (integer 5–600): stored per question; empty/absent means the session fallback (default 30 s). A non-numeric or out-of-range value is a `bad_seconds` warn-and-skip |

**Skipped rows are not stored anywhere** — there is no quarantine table and no
`quarantined` flag. The import report is their only record, so it must be explicit:
every skipped row is printed with its reason and its position (record index and physical
TSV line) so it can be found in the source file.

Output: a report (records read / imported / skipped, with counts per reason and per
chapter). The CLI writes it to its `--report` path; the server keeps it in memory and
serves it from `/api/admin/import/report.txt`.

The importer is the only writer of the question tables. The server only reads them.

---

## 9. Deployment

### 9.1 Artifact

One **self-contained** `bia-quiz` binary containing the SPA assets and the migrations, with SQLite bundled (glibc is linked dynamically),
plus a `bia.toml` and `bia.sqlite3` (questions, figures, results). No Node, no Postgres,
no Docker, no asset folder required at runtime.

### 9.2 Configuration (`bia.toml`, env overrides)

```toml
[server]
bind = "127.0.0.1:8080"
public_base_url = "https://quiz.asso-triple-a.org"   # used to build the join URL / QR
max_players = 60
join_code_length = 3                                # numeric room code (default 3)

[db]
path = "/var/lib/bia-quiz/bia.sqlite3"

[auth]
session_hours = 12

[prof]
user = "prof"
password = "..."        # plaintext in the root-only config; hashed at startup

[admin]
user = "admin"
password = "..."        # separate account; admin is a superset of prof

[limits]
requests_per_ip_per_minute = 240
```

Every key is overridable by an environment variable (`BIA_QUIZ_*`), so secrets never need
to sit in the file.

### 9.3 Caddy/TLS contract (Ansible role or owner)

The `caddy` Ansible role installs Caddy and writes the `Caddyfile`; an owner-managed
Caddy can use the same contract. The app listens on `127.0.0.1:8080` and exposes:

| Path | Purpose |
|------|---------|
| `/` | SPA (all client routes fall back to `index.html`) |
| `/ws` | WebSocket upgrade (teacher + students) |
| `/figures/*` | the question figures, read from SQLite |
| `/api/*` | small REST surface (room existence check, results CSV export) |
| `/healthz` | liveness for Caddy/systemd |

Sample `deploy/Caddyfile`:

```caddy
quiz.asso-triple-a.org {
    log {
        output stdout
        format console
    }
    encode gzip
    reverse_proxy 127.0.0.1:8080
}
```

Caddy handles the certificate and the WebSocket upgrade; `public_base_url` must be the
`https://` URL so the QR code points at `https://…/j/<code>`.

### 9.4 systemd

`deploy/bia-quiz.service` — a hardened unit (dedicated `bia-quiz` user,
`StateDirectory=bia-quiz`, `NoNewPrivileges`, `ProtectSystem=strict`,
`ReadWritePaths=/var/lib/bia-quiz`), with the install/upgrade steps documented
in `deploy/README.md`:

```
# first install
install binary + bia.toml
systemctl enable --now bia-quiz     # migrations run on first start
bia-import --tsv annales-bia.tsv --images data/figures   # first install only

# upgrade
cp bia.sqlite3 bia.sqlite3.bak-$(date +%F)   # migrations are forward-only (§7.5)
install the new binary
systemctl restart bia-quiz                   # pending migrations apply before bind
```

### 9.5 Ops

- Backups = copy `bia.sqlite3` (documented, with the WAL caveat).
- Logs to the journal; one structured line per session and per question.
- `/healthz` returns DB reachability.

---

## 10. Data

### 10.1 Where the questions come from

The canonical file is `site/static/annales-bia.tsv` in
[`Villemarque/bia-annales-database-csv`](https://github.com/Villemarque/bia-annales-database-csv);
the copy at the repository root is **byte-identical** (sha256 `b0974c4a…`, 348 522 bytes).
It holds **1 464 records**, 2015–2026, `answer` is an index into `choice_a..d`, 6 exam
subjects × 20 questions per year. Wording lives in
`content_verbatim` (exam wording) and `content_fixed` (cleaned wording, present on only
~77 rows). The file is a **quoted TSV**: some fields contain embedded newlines and
`"` quotes, so the importer must use a real quoted-TSV parser (§8 rule 1). With that
parser the file is clean — no duplicate `qid`, no malformed record, no bad answer index.
`mixed_choices` is a boolean (true = shuffle the choices at display time, §4); blank
counts as false.

### 10.2 Chapter titles (from the upstream README)

The indices in the `chapter` field are defined by the source repository, and they do not
match the subject numbers — the README warns *« les chapitres ne correspondent pas au
label »*. The importer bakes these 15 upstream titles into the binary:

| # | Title |
|---|-------|
| 0 | 1.1 Les aéronefs |
| 1 | 1.2 Instrumentation |
| 2 | 1.3 Moteurs |
| 3 | 2.1 La sustentation de l'aile |
| 4 | 2.2 Le vol stabilisé |
| 5 | 2.3 L'aérostation et le vol spatial |
| 6 | 3.1 L'atmosphère |
| 7 | 3.2 Les masses d'air et les fronts |
| 8 | 3.3 Les nuages |
| 9 | 3.4 Les vents |
| 10 | 3.5 Les phénomènes dangereux |
| 11 | 3.6 L'information météo |
| 12 | 4.1 Réglementation |
| 13 | 4.2 Sécurité des vols (SV) et facteurs humains (FH) |
| 14 | 4.3 Navigation |

The `chapter` field may hold a **comma-separated list** (one question can belong to
several chapters); the importer splits it and the UI shows the titles. The `subject`
field (0–5: météo, mécanique du vol, aéronef, navigation, histoire, anglais) drives the
teacher's subject groups; a question with no chapter appears under its subject's
`Sans chapitre` item.

### 10.3 What v1 actually has

Computed from the file with the §8 rules (all rows imported, valid `answer`), chapter 0
and chapterless questions counted separately:

| | records |
|---|---|
| total records | 1 464 |
| **usable in v1** | **1 464** (77 with `content_fixed`) |

| ch | title | usable |
|---:|---|---:|
| 0 | 1.1 Les aéronefs | 157 |
| 1 | 1.2 Instrumentation | 44 |
| 2 | 1.3 Moteurs | 44 |
| 3 | 2.1 La sustentation de l'aile | 122 |
| 4 | 2.2 Le vol stabilisé | 98 |
| 5 | 2.3 L'aérostation et le vol spatial | 23 |
| 6 | 3.1 L'atmosphère | 65 |
| 7 | 3.2 Les masses d'air et les fronts | 49 |
| 8 | 3.3 Les nuages | 35 |
| 9 | 3.4 Les vents | 38 |
| 10 | 3.5 Les phénomènes dangereux | 46 |
| 11 | 3.6 L'information météo | 11 |
| 12 | 4.1 Réglementation | 129 |
| 13 | 4.2 Sécurité des vols (SV) et facteurs humains (FH) | 25 |
| 14 | 4.3 Navigation | 90 |

488 questions have no chapter: Histoire (244, subject 4) and Anglais (244, subject 5).

All 190 figure-dependent rows resolve to **90 distinct `.jpeg` files** (each figure is
shared by about two questions).

Consequences to handle in the UI: a 10-question draw from chapter 11 alone is
impossible (11 usable) — the session must draw what exists and warn. The import report is
the authoritative count.

### 10.4 Figures

Figures are **included in v1**. Upstream `site/static/img-sujets/` holds **90 `.jpeg`
files named `<attachment_link>.jpeg`**; all 90 distinct `attachment_link` values in the
TSV match a file and there are no unreferenced images. They are vendored under
`data/figures/` (**7.1 MiB total**) as the import source, and `bia-import` stores each
distinct figure once in SQLite (`figures`: `id`, `mime`, `data BLOB`). The server loads
them into memory at startup and serves `/figures/<attachment_link>.jpeg` from there.

The projector renders the figure beside the question, scaled to fit; phones stay
buttons-only, so students look up at the projector exactly as they do for the question
text. A screen reader gets `alt="Figure associée à la question"` until the bank carries
real descriptions (§5.6). The runtime needs no asset folder: the database carries the
figures and the binary carries the SPA. The importer validates against `data/figures/`
at import time (§8 rule 5).

---

## 11. Security, privacy, risks

**Explicitly accepted by the owner, to be documented, not "fixed" silently:**

1. **The teacher console is open.** Anyone who knows the domain can start a room,
   read the question bank and see a running session's questions. There is no login.
2. **Results are retained indefinitely** with student nicknames (personal data) on a
   public server.
3. **Two config accounts** (`prof`, `admin`) protect `/prof` and `/admin`; passwords are
   plaintext in `bia.toml` (mode `0640`, owner root:bia-quiz) and hashed at startup. The
   teacher console remains open. Sessions live in memory and are lost on restart.
4. **No other abuse controls**: nickname spam, room flooding and quiz-start abuse are
   possible; the per-IP limit and the login-failure limiter apply.

Shipped anyway, because they are cheap and reduce the blast radius:

- Per-IP request rate limit (config `limits.requests_per_ip_per_minute`).
- Login-failure limiter (5 failures / 15 min per IP + username).
- `/healthz`.
- No student data collected beyond nickname, answer and timestamp; no email, no account.
- The `/prof` and `/admin` pages are marked `noindex`, and served with
  `Cache-Control: no-store`.

**Compliance note (to write into the README):** nicknames + results constitute
personal data under the GDPR; the retention decision above (§2 #15, indefinite) should
be revisited if this is ever hosted for a school, since a retention period, an
information notice and a deletion path would then be expected. The plan records the
decision; it does not silently mitigate it.

**Licence:** the code stays **AGPL‑3.0** (repo `LICENSE`). Because the app is offered
over a network, a footer link to the deployed source satisfies the AGPL's network-use
obligation. The question bank (`annales-bia.tsv`) and the figures are treated as
**separate content**, documented in a `NOTICE` file (origin, rightsholders, and the
fact that they are not covered by the AGPL grant on the code).

---

## 12. Testing and CI

- **Unit (Rust `core`)** — scorer curve (boundaries: `t=0`, `t=1`, wrong, late,
  changed answer), draw rule (no repeats, fewer questions than requested), the **shuffle
  mapping** (display position ↔ original index, correct answer mapped, false = no
  shuffle), room state machine transitions (start → reveal → leaderboard → next → end),
  tie handling.
- **Unit (importer)** — fixtures including a row with an embedded newline and quotes, a
  duplicate `qid`, a row with an empty chapter, a row with `chapter = "3,4"` (two membership rows), a
  row with chapter `0`, a row whose `attachment_link` file exists (imported) and one
  whose file is missing (skipped), an empty choice; asserts the skip warnings, the
  reasons, the membership rows and the report. A separate test asserts that every
  `attachment_link` in the TSV resolves to a file in `data/figures/`.
- **Router** — matching and params against the real `src/routes` tree, `+page.ts`
  `load()` results passed as `data`, unknown path → `+error.svelte`.
- **Auth** — login/logout, the role matrix (`prof` vs `admin`), page redirect vs API 401,
  session invalidation after a password-hash change, login rate limit, cross-origin POST
  rejection.
- **E2E** — one integration test that starts the real axum server, connects **two
  WebSocket clients** (a teacher and a student), plays a full question, and asserts the
  scoring and the reveal/leaderboard sequence.
- **Frontend** — `pnpm check` (types) + `pnpm build`, plus a couple of component tests
  for the answer button (locking/replacing behaviour).
- **Migrations** — apply and then revert every migration against a temporary SQLite; CI
  also fails if the committed `schema.rs` is stale (`diesel migration run --locked-schema`).
- **CI** — GitHub Actions: `cargo fmt --check`, `cargo clippy`, `cargo test`,
  `pnpm check`, `pnpm build`, plus a sha256 check that `annales-bia.tsv` still matches
  the pinned upstream file. No deploy job.

---

## 13. Milestones

| # | Milestone | Exit criterion |
|---|-----------|----------------|
| M0 | Import | `bia-import` loads all 1 464 questions (chapter 0 and Histoire/Anglais included, figures resolved), report matches §10.3, every rejected row is listed with a reason |
| M1 | Core | scorer + room state machine + protocol types, unit-tested |
| M2 | Server | axum serves the embedded SPA, `/ws` drives a room, `/healthz`, rate limit, axum-login sessions for `/prof` and `/admin` |
| M3 | Teacher UI | configure, lobby with QR + code, question, two reveal screens, podium, keyboard-operable |
| M4 | Student UI | `/j/<code>`, nickname + token, four buttons, change-until-timeout, right/wrong, final result |
| M5 | Results | session list, detail, CSV export |
| M6 | Deploy | static binary, systemd unit, sample Caddyfile, `deploy/README.md`, backup notes |
| M7 | Hardening | E2E test, CI green, a11y pass, charte compliance pass |

### Backlog (explicitly out of v1)

Teams · self-paced/homework · practice mode with corrections · question editor ·
accounts/SSO · sound · Postgres · multiple concurrent rooms · subject filter ·
adaptive difficulty.

---

## 14. Open questions (to confirm)

These are the only places where the plan proposes rather than records.

1. **Sigmoid parameters** — `k` and midpoint `m` (proposal: `k = 8`, `m = 0.5`), and
   whether rounding plus a small floor is wanted (§6).
2. **Reveal #1 content** — besides highlighting the correct answer, do we also show the
   per-choice answer counts on the projector? (§5.1)
3. **Chapter 0** — chapter `0` (`1.1 Les aéronefs`, 157 records) is now **included**,
   correcting an earlier decision that rested on my wrong "chapter 0 = unclassified"
   premise. Figures are in v1 per your call; confirm chapter 0 (§10.2–§10.3).
4. **Defaults** — 10 questions and 30 seconds as the session fallback (§4).
5. **Figure-reference safeguard** — do we also skip questions whose wording says
   "ci-contre" without an `attachment_link` (§8 rule 8)? Recommended: yes.
6. **Button colour mapping** — which charte colours go on the four shapes (§5.3).
7. **Build helpers** — `justfile` vs `Makefile` vs plain README commands (§7.2).
8. **Shuffling safety** — some choices may reference each other ("aucune de ces
   réponses", "les réponses A et C"). For questions flagged `mixed_choices = TRUE` whose
   choices contain such references, should the importer force no-shuffle and warn?
   Recommended: yes.
9. **Teacher console** — keep `/teacher` open (current decision) or protect it with the
   same login now?
10. **Session lifetime** — 12 h absolute (proposed) or sliding?

---

## 15. `/admin` — dynamic question import (implemented in v1)

Goal: let the professor import or refresh the question bank from the browser, so the
separate `bia-import` step is no longer required for normal operation. The CLI remains as
a thin wrapper for bootstrap, CI and recovery, but it is no longer part of the teaching
workflow.

### 15.1 Scope (v1)

- A `/admin` page behind its **own** `admin` account (axum-login sessions, separate from
  `prof`, §7.7) with:
  - current database stats: questions, active questions, figures, chapters, file size;
  - import from the **configured upstream URL** only (no local folder, no upload);
  - live progress and the final report (imported / skipped / figures, reasons, per
    chapter, skipped rows);
  - a refusal to start while a game room is active or another import is running.
- Not in scope: editing a single question, deleting one question, per-question image
  upload, multiple admin accounts, scheduled imports.

### 15.2 UX (French, charte)

`/admin` is a single page, separate from `/prof` (results):

1. **État** — questions total/active, figures, chapters and DB size.
2. **Source** — the upstream URL, shown read-only from `[import].upstream_url`.
3. **Lancer l'import** — disabled while a room is active, during an import, or when no
   source is selected. The download uses a temporary working directory.
4. **Progression** — `Téléchargement…` → `Import…` → `Terminé` / `Échec`, with the
   processed row count, start/end timestamps and an error message.
5. **Rapport** — the import counters and the skipped-row table, plus a link to download
   the full report.

After a successful import the page shows the new counts immediately; no restart is needed
(the server reloads its figure cache, §15.4).

### 15.3 HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/admin` | SPA page (admin session) |
| `GET` | `/api/admin/stats` | counts, DB size, upstream URL |
| `POST` | `/api/admin/import` | start the upstream import (no body) → `202` |
| `GET` | `/api/admin/import` | job state + report (polled every 1.5 s) |
| `GET` | `/api/admin/import/report.txt` | full report download |

Rules:

- all routes require an authenticated session; `/api/admin/*` also requires the `admin`
  role (§7.7);
- mutating routes reject requests whose `Sec-Fetch-Site` / `Origin` is not same-origin;
- `409 Conflict` if an import is running or a room is active;
- only one import job exists at a time (no queue);
- errors are returned as `{ "message": "…" }` in French.

### 15.4 Architecture

**Extract the importer into a library.** Today `crates/importer` is a binary that owns its
connection and migrations. Split it:

```
crates/importer/src/lib.rs     fetch(url, tsv, images) -> FetchOutcome
                               import(conn, options, progress) -> Report
                               extract_archive(reader, tsv, images)
crates/importer/src/main.rs    thin CLI over the library (chapters flag optional)
crates/server/src/admin.rs     routes + job
```

`import` takes `&mut SqliteConnection`, never opens a connection and never runs
migrations; the server already applied them at startup. The CLI keeps doing both.

**Job manager.** One `Arc<Mutex<ImportJob>>` in `AppState`:

```
Idle → Downloading → Importing → Done(Report) | Failed(String)
```

- `POST` validates (room idle, job idle), spawns `spawn_blocking`, returns `202`.
- The job opens a **second SQLite connection** (WAL) with the same pragmas, so the
  server's main connection stays free for reads. SQLite still allows only one writer; the
  import runs in **one transaction**, so the DB is never half-imported.
- Progress is reported through a callback (rows imported) into the job state; the page
  polls the GET endpoint.
- The download (TSV + figures) uses a temporary directory; the embedded chapter seed is
  applied during the transaction. On success: commit, build the report, then reload the
  figure cache from SQLite.
- On failure: roll back, keep the previous bank, expose the error.

**Figure cache.** `AppState.figures` moves from `Arc<HashMap<…>>` to
`Arc<RwLock<HashMap<…>>>` (or an `arc-swap`), with `reload_figures()` called after a
successful import. Downloaded images are the import source only; afterwards they exist
only as SQLite blobs.

**Room guard.** An import is refused while a room exists (`state.room` is `Some` and
alive). This keeps the live draw and the figure cache stable; teachers end the session
before refreshing the bank.

### 15.5 Data model

- **`questions.active BOOL NOT NULL DEFAULT 1`** (new migration). Import sets every row
  to `active = 0`, upserts the new TSV with `active = 1`, all inside the transaction. Draw
  selects `active = 1`. Retired questions stay for past sessions and results, so the
  `session_questions` foreign key is never broken. This replaces the current
  `forget_question` delete path, which can fail on a referenced row.
- **Full sync is the only v1 mode**: every import deactivates missing questions; the
  report reports their count.
- **`import_runs`** (optional, not in v1): `id, started_at, ended_at, source, imported,
  skipped, figures, status, report_json`. Would add a last-import view and an audit trail.
- `figures` is unchanged: upsert by id; the import never removes a figure still referenced
  by a question (active or retired).
- A new migration is added; the applied ones are never edited (§7.5).

### 15.6 Security

- **Separate `admin` account**, never the `prof` one; `/admin` and `/api/admin/*` require
  the `admin` role; **same-origin check** on POST (the session cookie is `SameSite=Lax`).
- Fetch: the upstream URL is read from `[import] upstream_url`, never from the browser;
  HTTPS only, request timeout, max download size. No arbitrary-URL surface, so no SSRF
  from the admin UI.
- One job at a time; a live room blocks imports; the whole import is one transaction.
- The import endpoint is not exposed to students; `/admin` is noindex/no-store like
  `/prof`.

### 15.7 Configuration

```toml
[admin]
user = "admin"
password = "..."        # separate from [prof]; hashed at startup

[import]
upstream_url = "https://github.com/Villemarque/bia-annales-database-csv/archive/refs/heads/master.tar.gz"
```

### 15.8 Tests

- **Library**: existing importer tests still pass; add `import` with a transaction and the
  `active` flag (removed questions become inactive, not deleted).
- **Job**: idle → downloading → importing → done; a second POST returns 409; a failing
  import rolls back and reports the error.
- **HTTP**: 401 without auth, 403 cross-origin, 409 with a live room, import + poll +
  report, cache reload (a new figure is served immediately).
- **End-to-end**: start the server on a fixture DB, POST an import, poll, assert the
  question/figure counts and that `/figures/<id>.jpeg` reflects the new image.

### 15.9 Rollout

1. Extract the importer library; the CLI becomes a wrapper (no behavior change).
2. Add the `active` column migration and the `active` handling in the importer and draw.
3. Add the job manager and API.
4. Add the `/admin` SPA page and its poll loop.
5. Update README/deploy: start the server, open `/admin`, import; the CLI is optional.

### 15.10 Decisions (locked)

1. **Separate `admin` account** via axum-login, distinct from `prof`; the admin role is a
   superset of prof (proposed).
2. **Upstream URL only** — the local folder and uploads are removed.
3. **Full sync is the only mode** (missing questions are deactivated, never deleted).
4. **Imports are refused while a room is live** (no queue).
5. **The CLI stays** as a small wrapper over the library.
6. **The upstream URL comes from config only**; the admin UI never accepts a URL.
