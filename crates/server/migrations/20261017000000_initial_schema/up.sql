CREATE TABLE chapters (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    number  INTEGER NOT NULL UNIQUE,
    title   TEXT    NOT NULL
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
    attachment_link TEXT
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
