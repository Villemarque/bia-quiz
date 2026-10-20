//! WebSocket protocol shared by the server and the SPA.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Lobby,
    Question,
    RevealAnswer,
    RevealLeaderboard,
    Podium,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPick {
    pub subject: i32,
    /// `None` selects questions from this subject that have no chapter.
    pub chapter: Option<i32>,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    #[serde(rename = "teacher.create")]
    TeacherCreate {
        chapters: Vec<ChapterPick>,
        seconds: u32,
    },
    #[serde(rename = "teacher.start_question")]
    TeacherStartQuestion,
    #[serde(rename = "teacher.force_reveal")]
    TeacherForceReveal,
    #[serde(rename = "teacher.show_leaderboard")]
    TeacherShowLeaderboard,
    #[serde(rename = "teacher.next")]
    TeacherNext,
    #[serde(rename = "teacher.pause")]
    TeacherPause,
    #[serde(rename = "teacher.resume")]
    TeacherResume,
    #[serde(rename = "teacher.skip")]
    TeacherSkip,
    #[serde(rename = "teacher.end")]
    TeacherEnd,
    #[serde(rename = "student.join")]
    StudentJoin { nickname: String },
    #[serde(rename = "student.answer")]
    StudentAnswer { choice: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceView {
    pub text: String,
    /// One of `triangle`, `diamond`, `circle`, `square` (by displayed position).
    pub shape: String,
    /// `A`, `B`, `C`, `D` (by displayed position).
    pub letter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreView {
    pub nickname: String,
    pub points: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    pub nickname: String,
    pub points: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    #[serde(rename = "hello")]
    Hello { role: String, server_now: i64 },

    #[serde(rename = "room.created")]
    RoomCreated {
        code: String,
        join_url: String,
        available: usize,
        requested: usize,
        warning: Option<String>,
    },

    #[serde(rename = "room.state")]
    RoomState {
        phase: Phase,
        code: String,
        player_count: usize,
        players: Vec<PlayerView>,
    },

    #[serde(rename = "player.welcome")]
    PlayerWelcome { token: String, player_id: i32 },

    #[serde(rename = "question.start")]
    QuestionStart {
        index: usize,
        total: usize,
        text: String,
        figure_url: Option<String>,
        choices: Vec<ChoiceView>,
        ends_at: i64,
        server_now: i64,
        seconds: u32,
    },

    #[serde(rename = "answers.count")]
    AnswersCount { count: usize, total: usize },

    #[serde(rename = "paused")]
    Paused {
        paused: bool,
        remaining_ms: Option<i64>,
    },

    #[serde(rename = "question.reveal")]
    QuestionReveal {
        correct_index: usize,
        counts: [usize; 4],
        correct_text: String,
    },

    #[serde(rename = "question.result")]
    QuestionResult { correct: bool },

    #[serde(rename = "leaderboard")]
    Leaderboard { top: Vec<ScoreView> },

    #[serde(rename = "session.end")]
    SessionEnd {
        podium: Vec<ScoreView>,
        top: Vec<ScoreView>,
        my_rank: Option<usize>,
        my_points: Option<i64>,
    },

    #[serde(rename = "answer.saved")]
    AnswerSaved { choice: usize },

    #[serde(rename = "error")]
    Error { message: String },
}

pub const SHAPES: [&str; 4] = ["triangle", "diamond", "circle", "square"];
pub const LETTERS: [&str; 4] = ["A", "B", "C", "D"];

impl ChoiceView {
    pub fn at(text: &str, position: usize) -> Self {
        Self {
            text: text.to_string(),
            shape: SHAPES[position % 4].to_string(),
            letter: LETTERS[position % 4].to_string(),
        }
    }
}
