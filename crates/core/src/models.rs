use diesel::prelude::*;
use serde::Serialize;

use crate::schema::*;

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = chapters)]
pub struct Chapter {
    pub id: i32,
    pub number: i32,
    pub title: String,
}

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = questions)]
pub struct Question {
    pub qid: String,
    pub year: i32,
    pub subject: i32,
    pub text: String,
    pub used_fixed: bool,
    pub choice_a: String,
    pub choice_b: String,
    pub choice_c: String,
    pub choice_d: String,
    pub answer_idx: i32,
    pub mixed_choices: bool,
    pub attachment_link: Option<String>,
    pub active: bool,
    pub seconds: Option<i32>,
}

impl Question {
    /// The four choices in the TSV order (A, B, C, D).
    pub fn choices(&self) -> [&str; 4] {
        [
            self.choice_a.as_str(),
            self.choice_b.as_str(),
            self.choice_c.as_str(),
            self.choice_d.as_str(),
        ]
    }

    pub fn figure_url(&self) -> Option<String> {
        self.attachment_link
            .as_ref()
            .map(|id| format!("/figures/{id}.jpeg"))
    }
}

#[derive(Insertable, AsChangeset, Debug)]
#[diesel(table_name = questions)]
pub struct NewQuestion<'a> {
    pub qid: &'a str,
    pub year: i32,
    pub subject: i32,
    pub text: &'a str,
    pub used_fixed: bool,
    pub choice_a: &'a str,
    pub choice_b: &'a str,
    pub choice_c: &'a str,
    pub choice_d: &'a str,
    pub answer_idx: i32,
    pub mixed_choices: bool,
    pub attachment_link: Option<&'a str>,
    pub active: bool,
    pub seconds: Option<i32>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = chapters)]
pub struct NewChapter {
    pub number: i32,
    pub title: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = question_chapters)]
pub struct NewQuestionChapter {
    pub question_qid: String,
    pub chapter_id: i32,
}

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = figures)]
pub struct FigureRow {
    pub id: String,
    pub mime: String,
    pub data: Vec<u8>,
}

#[derive(Insertable, AsChangeset, Debug)]
#[diesel(table_name = figures)]
pub struct NewFigure<'a> {
    pub id: &'a str,
    pub mime: &'a str,
    pub data: &'a [u8],
}

#[derive(Queryable, Selectable, Clone, Debug, Serialize)]
#[diesel(table_name = sessions)]
pub struct Session {
    pub id: i32,
    pub code: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub question_count: i32,
    pub seconds_per_question: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub code: &'a str,
    pub started_at: i64,
    pub question_count: i32,
    pub seconds_per_question: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = session_chapters)]
pub struct NewSessionChapter {
    pub session_id: i32,
    pub chapter_id: i32,
}

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = session_questions)]
pub struct SessionQuestion {
    pub id: i32,
    pub session_id: i32,
    pub position: i32,
    pub qid: String,
    pub display_order: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = session_questions)]
pub struct NewSessionQuestion<'a> {
    pub session_id: i32,
    pub position: i32,
    pub qid: &'a str,
    pub display_order: &'a str,
}

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = players)]
pub struct PlayerRow {
    pub id: i32,
    pub session_id: i32,
    pub token: String,
    pub nickname: String,
    pub joined_at: i64,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = players)]
pub struct NewPlayer<'a> {
    pub session_id: i32,
    pub token: &'a str,
    pub nickname: &'a str,
    pub joined_at: i64,
}

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = answers)]
pub struct AnswerRow {
    pub id: i32,
    pub session_question_id: i32,
    pub player_id: i32,
    pub choice_idx: i32,
    pub is_correct: bool,
    pub points: i32,
    pub answered_at: i64,
    pub ms_elapsed: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = answers)]
pub struct NewAnswer {
    pub session_question_id: i32,
    pub player_id: i32,
    pub choice_idx: i32,
    pub is_correct: bool,
    pub points: i32,
    pub answered_at: i64,
    pub ms_elapsed: i32,
}

/// One answer as the room keeps it in memory (one row per player per question).
#[derive(Clone, Debug)]
pub struct AnswerState {
    pub display_choice: usize,
    pub original_choice: usize,
    pub is_correct: bool,
    pub points: i32,
    pub answered_at: i64,
    pub ms_elapsed: i32,
}
