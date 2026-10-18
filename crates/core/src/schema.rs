// @generated automatically by Diesel CLI.
// Source of truth: crates/server/migrations/*_initial_schema/up.sql.
// Regenerate with `diesel print-schema`; do not hand-edit.

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
