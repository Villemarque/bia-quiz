use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use bia_core::draw::draw_per_chapter;
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

pub async fn chapters(db: &Db) -> Result<Vec<ChapterCount>> {
    db.call(|conn| {
        let rows = diesel::sql_query(
            "SELECT c.number AS number, c.title AS title, \
                    COUNT(DISTINCT q.qid) AS question_count \
             FROM chapters c \
             LEFT JOIN question_chapters qc ON qc.chapter_id = c.id \
             LEFT JOIN questions q ON q.qid = qc.question_qid AND q.active = 1 \
             GROUP BY c.id ORDER BY c.number",
        )
        .load::<ChapterCount>(conn)?;
        Ok(rows)
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

/// Draw up to each chapter's requested count, never repeating a question that
/// belongs to several selected chapters. Returns the questions in draw order
/// plus the number of unique active questions in the selected chapters.
pub async fn draw(db: &Db, picks: &[(i32, usize)]) -> Result<(Vec<Question>, usize)> {
    let picks = picks.to_vec();
    let numbers: Vec<i32> = picks.iter().map(|(number, _)| *number).collect();
    let (ids, available) = db
        .call(move |conn| {
            let rows = question_chapters::table
                .inner_join(chapters::table)
                .inner_join(questions::table)
                .filter(chapters::number.eq_any(&numbers))
                .filter(questions::active.eq(true))
                .select((chapters::number, question_chapters::question_qid))
                .load::<(i32, String)>(conn)?;
            let mut unique: Vec<&str> = rows.iter().map(|(_, qid)| qid.as_str()).collect();
            unique.sort_unstable();
            unique.dedup();
            let available = unique.len();
            let chosen = draw_per_chapter(rows, &picks, &mut thread_rng());
            Ok((chosen, available))
        })
        .await?;

    if ids.is_empty() {
        bail!("aucune question disponible pour ces chapitres");
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
