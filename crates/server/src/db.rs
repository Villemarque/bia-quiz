use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use bia_core::draw::draw_per_pick;
use bia_core::models::*;
use bia_core::schema::*;
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Bool, Integer, Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use rand::thread_rng;
use serde::Serialize;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<SqliteConnection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = open_connection(path)?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("migration failed: {e}"))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run a blocking Diesel closure on the single connection.
    pub async fn call<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut SqliteConnection) -> Result<T> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().map_err(|_| anyhow!("database lock poisoned"))?;
            f(&mut guard)
        })
        .await?
    }
}

/// Open a second connection with the same pragmas, without running migrations.
pub fn open_connection(path: &Path) -> Result<SqliteConnection> {
    let mut conn = SqliteConnection::establish(&path.to_string_lossy())
        .with_context(|| format!("cannot open SQLite database {}", path.display()))?;
    conn.batch_execute(
        "PRAGMA foreign_keys = ON;\n\
         PRAGMA journal_mode = WAL;\n\
         PRAGMA busy_timeout = 5000;\n\
         PRAGMA synchronous = NORMAL;",
    )
    .context("cannot set SQLite pragmas")?;
    Ok(conn)
}

const SUBJECTS: [(i32, &str); 6] = [
    (2, "Aéronefs"),
    (1, "Mécanique du vol"),
    (0, "Météo"),
    (3, "Navigation"),
    (4, "Histoire"),
    (5, "Anglais"),
];

#[derive(Serialize, Clone, Debug)]
pub struct SubjectCatalog {
    pub subject: i32,
    pub title: String,
    pub question_count: i64,
    pub chapters: Vec<ChapterCount>,
    pub no_chapter_count: i64,
}

#[derive(QueryableByName)]
struct SubjectChapterRow {
    #[diesel(sql_type = Integer)]
    subject: i32,
    #[diesel(sql_type = Integer)]
    number: i32,
    #[diesel(sql_type = Text)]
    title: String,
    #[diesel(sql_type = BigInt)]
    question_count: i64,
}

#[derive(QueryableByName)]
struct SubjectCountRow {
    #[diesel(sql_type = Integer)]
    subject: i32,
    #[diesel(sql_type = BigInt)]
    question_count: i64,
}

pub async fn chapters(db: &Db) -> Result<Vec<SubjectCatalog>> {
    db.call(|conn| {
        let chapter_rows: Vec<SubjectChapterRow> = diesel::sql_query(
            "SELECT q.subject AS subject, c.number AS number, c.title AS title, \
                    COUNT(DISTINCT q.qid) AS question_count \
             FROM questions q \
             JOIN question_chapters qc ON qc.question_qid = q.qid \
             JOIN chapters c ON c.id = qc.chapter_id \
             WHERE q.active = 1 \
             GROUP BY q.subject, c.number, c.title \
             ORDER BY q.subject, c.number",
        )
        .load(conn)?;

        let no_chapter_rows: Vec<SubjectCountRow> = diesel::sql_query(
            "SELECT q.subject AS subject, COUNT(*) AS question_count \
             FROM questions q \
             WHERE q.active = 1 \
               AND NOT EXISTS (SELECT 1 FROM question_chapters qc \
                               WHERE qc.question_qid = q.qid) \
             GROUP BY q.subject",
        )
        .load(conn)?;

        let total_rows: Vec<SubjectCountRow> = diesel::sql_query(
            "SELECT subject, COUNT(*) AS question_count \
             FROM questions WHERE active = 1 GROUP BY subject",
        )
        .load(conn)?;

        let mut by_subject: HashMap<i32, SubjectCatalog> = SUBJECTS
            .iter()
            .map(|(number, title)| {
                (
                    *number,
                    SubjectCatalog {
                        subject: *number,
                        title: (*title).to_string(),
                        question_count: 0,
                        chapters: Vec::new(),
                        no_chapter_count: 0,
                    },
                )
            })
            .collect();

        for row in total_rows {
            if let Some(subject) = by_subject.get_mut(&row.subject) {
                subject.question_count = row.question_count;
            }
        }
        for row in chapter_rows {
            if let Some(subject) = by_subject.get_mut(&row.subject) {
                subject.chapters.push(ChapterCount {
                    number: row.number,
                    title: row.title,
                    question_count: row.question_count,
                });
            }
        }
        for row in no_chapter_rows {
            if let Some(subject) = by_subject.get_mut(&row.subject) {
                subject.no_chapter_count = row.question_count;
            }
        }

        Ok(SUBJECTS
            .iter()
            .filter_map(|(number, _)| by_subject.remove(number))
            .collect())
    })
    .await
}

#[derive(QueryableByName, Serialize, Clone, Debug)]
pub struct ChapterCount {
    #[diesel(sql_type = Integer)]
    pub number: i32,
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = BigInt)]
    pub question_count: i64,
}

#[derive(QueryableByName)]
struct QuestionChapterRow {
    #[diesel(sql_type = Text)]
    qid: String,
    #[diesel(sql_type = Integer)]
    subject: i32,
    #[diesel(sql_type = Nullable<Integer>)]
    chapter: Option<i32>,
}

/// Draw up to each pick's requested count, never repeating a question.
/// A pick is `(subject, chapter or None for no chapter, count)`.
pub async fn draw(db: &Db, picks: &[(i32, Option<i32>, usize)]) -> Result<(Vec<Question>, usize)> {
    let picks = picks.to_vec();
    let (ids, available) = db
        .call(move |conn| {
            let rows: Vec<QuestionChapterRow> = diesel::sql_query(
                "SELECT q.qid AS qid, q.subject AS subject, c.number AS chapter \
                 FROM questions q \
                 LEFT JOIN question_chapters qc ON qc.question_qid = q.qid \
                 LEFT JOIN chapters c ON c.id = qc.chapter_id \
                 WHERE q.active = 1",
            )
            .load(conn)?;

            let mut by_qid: HashMap<String, (i32, Vec<i32>)> = HashMap::new();
            for row in rows {
                let entry = by_qid
                    .entry(row.qid)
                    .or_insert_with(|| (row.subject, Vec::new()));
                if let Some(chapter) = row.chapter {
                    entry.1.push(chapter);
                }
            }

            let mut candidates: Vec<(usize, String)> = Vec::new();
            let mut unique = HashSet::new();
            for (index, (subject, chapter, _)) in picks.iter().enumerate() {
                for (qid, (question_subject, chapters)) in &by_qid {
                    if question_subject != subject {
                        continue;
                    }
                    let matched = match chapter {
                        Some(number) => chapters.contains(number),
                        None => chapters.is_empty(),
                    };
                    if matched {
                        candidates.push((index, qid.clone()));
                        unique.insert(qid.clone());
                    }
                }
            }
            let available = unique.len();
            let quotas: Vec<(usize, usize)> = picks
                .iter()
                .enumerate()
                .map(|(index, (_, _, count))| (index, *count))
                .collect();
            let chosen = draw_per_pick(candidates, &quotas, &mut thread_rng());
            Ok((chosen, available))
        })
        .await?;

    if ids.is_empty() {
        bail!("aucune question disponible pour cette sélection");
    }

    let chosen = ids.clone();
    let mut loaded = db
        .call(move |conn| {
            Ok(questions::table
                .filter(questions::qid.eq_any(&chosen))
                .load::<Question>(conn)?)
        })
        .await?;

    // Keep the random draw order.
    loaded.sort_by_key(|q| ids.iter().position(|id| id == &q.qid).unwrap_or(usize::MAX));
    Ok((loaded, available))
}

pub async fn chapter_ids(db: &Db, numbers: &[i32]) -> Result<Vec<i32>> {
    let numbers = numbers.to_vec();
    db.call(move |conn| {
        Ok(chapters::table
            .filter(chapters::number.eq_any(&numbers))
            .select(chapters::id)
            .load::<i32>(conn)?)
    })
    .await
}

pub async fn load_figures(db: &Db) -> Result<Vec<FigureRow>> {
    db.call(|conn| Ok(figures::table.load::<FigureRow>(conn)?))
        .await
}

pub async fn session(db: &Db, id: i32) -> Result<Option<Session>> {
    db.call(move |conn| {
        Ok(sessions::table
            .filter(sessions::id.eq(id))
            .first::<Session>(conn)
            .optional()?)
    })
    .await
}

pub struct NewSessionData<'a> {
    pub code: &'a str,
    pub started_at: i64,
    pub question_count: i32,
    pub seconds: i32,
    pub chapter_ids: &'a [i32],
}

pub async fn create_session(db: &Db, data: NewSessionData<'_>) -> Result<i32> {
    let code = data.code.to_string();
    let chapter_ids = data.chapter_ids.to_vec();
    db.call(move |conn| {
        diesel::insert_into(sessions::table)
            .values(NewSession {
                code: &code,
                started_at: data.started_at,
                question_count: data.question_count,
                seconds_per_question: data.seconds,
            })
            .execute(conn)?;
        let id: i32 =
            diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()")).get_result(conn)?;
        for chapter_id in chapter_ids {
            diesel::insert_into(session_chapters::table)
                .values(NewSessionChapter {
                    session_id: id,
                    chapter_id,
                })
                .on_conflict((session_chapters::session_id, session_chapters::chapter_id))
                .do_nothing()
                .execute(conn)?;
        }
        Ok(id)
    })
    .await
}

pub async fn add_session_question(
    db: &Db,
    session_id: i32,
    position: i32,
    qid: &str,
    display_order: &str,
) -> Result<i32> {
    let qid = qid.to_string();
    let order = display_order.to_string();
    db.call(move |conn| {
        diesel::insert_into(session_questions::table)
            .values(NewSessionQuestion {
                session_id,
                position,
                qid: &qid,
                display_order: &order,
            })
            .execute(conn)?;
        let id: i32 =
            diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()")).get_result(conn)?;
        Ok(id)
    })
    .await
}

pub async fn remove_session_question(db: &Db, id: i32) -> Result<()> {
    db.call(move |conn| {
        diesel::delete(session_questions::table.filter(session_questions::id.eq(id)))
            .execute(conn)?;
        Ok(())
    })
    .await
}

pub async fn add_player(
    db: &Db,
    session_id: i32,
    token: &str,
    nickname: &str,
    joined_at: i64,
) -> Result<i32> {
    let token = token.to_string();
    let nickname = nickname.to_string();
    db.call(move |conn| {
        diesel::insert_into(players::table)
            .values(NewPlayer {
                session_id,
                token: &token,
                nickname: &nickname,
                joined_at,
            })
            .execute(conn)?;
        let id: i32 =
            diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()")).get_result(conn)?;
        Ok(id)
    })
    .await
}

pub async fn find_player_by_token(
    db: &Db,
    session_id: i32,
    token: &str,
) -> Result<Option<PlayerRow>> {
    let token = token.to_string();
    db.call(move |conn| {
        Ok(players::table
            .filter(players::session_id.eq(session_id))
            .filter(players::token.eq(&token))
            .first::<PlayerRow>(conn)
            .optional()?)
    })
    .await
}

pub async fn upsert_answer(db: &Db, answer: NewAnswer) -> Result<()> {
    db.call(move |conn| {
        diesel::insert_into(answers::table)
            .values(&answer)
            .on_conflict((answers::session_question_id, answers::player_id))
            .do_update()
            .set((
                answers::choice_idx.eq(answer.choice_idx),
                answers::is_correct.eq(answer.is_correct),
                answers::points.eq(answer.points),
                answers::answered_at.eq(answer.answered_at),
                answers::ms_elapsed.eq(answer.ms_elapsed),
            ))
            .execute(conn)?;
        Ok(())
    })
    .await
}

pub async fn finish_session(db: &Db, session_id: i32, ended_at: i64) -> Result<()> {
    db.call(move |conn| {
        diesel::update(sessions::table.filter(sessions::id.eq(session_id)))
            .set(sessions::ended_at.eq(ended_at))
            .execute(conn)?;
        Ok(())
    })
    .await
}

pub async fn session_question_id(db: &Db, session_id: i32, position: i32) -> Result<Option<i32>> {
    db.call(move |conn| {
        Ok(session_questions::table
            .filter(session_questions::session_id.eq(session_id))
            .filter(session_questions::position.eq(position))
            .select(session_questions::id)
            .first::<i32>(conn)
            .optional()?)
    })
    .await
}

#[derive(QueryableByName, Serialize, Clone, Debug)]
pub struct SessionListRow {
    #[diesel(sql_type = Integer)]
    pub id: i32,
    #[diesel(sql_type = Text)]
    pub code: String,
    #[diesel(sql_type = BigInt)]
    pub started_at: i64,
    #[diesel(sql_type = Nullable<BigInt>)]
    pub ended_at: Option<i64>,
    #[diesel(sql_type = Integer)]
    pub question_count: i32,
    #[diesel(sql_type = Integer)]
    pub seconds_per_question: i32,
    #[diesel(sql_type = BigInt)]
    pub players: i64,
}

pub async fn list_sessions(db: &Db) -> Result<Vec<SessionListRow>> {
    db.call(|conn| {
        Ok(diesel::sql_query(
            "SELECT s.id AS id, s.code AS code, s.started_at AS started_at, \
                    s.ended_at AS ended_at, s.question_count AS question_count, \
                    s.seconds_per_question AS seconds_per_question, \
                    (SELECT COUNT(*) FROM players p WHERE p.session_id = s.id) AS players \
             FROM sessions s ORDER BY s.started_at DESC",
        )
        .load::<SessionListRow>(conn)?)
    })
    .await
}

#[derive(QueryableByName, Serialize, Clone, Debug)]
pub struct SessionQuestionRow {
    #[diesel(sql_type = Integer)]
    pub position: i32,
    #[diesel(sql_type = Text)]
    pub qid: String,
    #[diesel(sql_type = Text)]
    pub text: String,
    #[diesel(sql_type = Text)]
    pub display_order: String,
    #[diesel(sql_type = Integer)]
    pub answer_idx: i32,
}

#[derive(QueryableByName, Serialize, Clone, Debug)]
pub struct SessionAnswerRow {
    #[diesel(sql_type = Text)]
    pub nickname: String,
    #[diesel(sql_type = Integer)]
    pub position: i32,
    #[diesel(sql_type = Integer)]
    pub choice_idx: i32,
    #[diesel(sql_type = Bool)]
    pub is_correct: bool,
    #[diesel(sql_type = Integer)]
    pub points: i32,
}

pub async fn session_questions(db: &Db, session_id: i32) -> Result<Vec<SessionQuestionRow>> {
    db.call(move |conn| {
        Ok(diesel::sql_query(
            "SELECT sq.position AS position, sq.qid AS qid, q.text AS text, \
                    sq.display_order AS display_order, q.answer_idx AS answer_idx \
             FROM session_questions sq JOIN questions q ON q.qid = sq.qid \
             WHERE sq.session_id = ? ORDER BY sq.position",
        )
        .bind::<Integer, _>(session_id)
        .load::<SessionQuestionRow>(conn)?)
    })
    .await
}

pub async fn session_answers(db: &Db, session_id: i32) -> Result<Vec<SessionAnswerRow>> {
    db.call(move |conn| {
        Ok(diesel::sql_query(
            "SELECT p.nickname AS nickname, sq.position AS position, \
                    a.choice_idx AS choice_idx, a.is_correct AS is_correct, a.points AS points \
             FROM answers a \
             JOIN players p ON p.id = a.player_id \
             JOIN session_questions sq ON sq.id = a.session_question_id \
             WHERE sq.session_id = ? \
             ORDER BY sq.position, p.nickname",
        )
        .bind::<Integer, _>(session_id)
        .load::<SessionAnswerRow>(conn)?)
    })
    .await
}
