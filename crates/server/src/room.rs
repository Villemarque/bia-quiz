//! The live room actor: one room, one task, one source of truth for time.
//!
//! All commands arrive on one channel, so the room never needs a lock. Database
//! writes are awaited through `Db::call` (a blocking task on one SQLite connection).

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bia_core::draw::{identity_order, order_to_string, shuffle_order};
use bia_core::models::{AnswerState, NewAnswer, Question};
use bia_core::protocol::{ChoiceView, Phase, PlayerAnswerView, PlayerView, ScoreView, ServerMsg};
use rand::Rng;
use tokio::sync::{mpsc, oneshot};

use crate::db::{self, Db};

/// Messages sent to one client. Bounded so a slow socket cannot grow memory.
pub type ClientTx = mpsc::Sender<ServerMsg>;
pub const CLIENT_QUEUE: usize = 64;
const ROOM_QUEUE: usize = 256;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy)]
pub enum TeacherCmd {
    StartQuestion,
    ForceReveal,
    ShowLeaderboard,
    Next,
    Pause,
    Resume,
    Skip,
    End,
}

pub enum Cmd {
    TeacherAttach(ClientTx),
    Teacher(TeacherCmd),
    StudentJoin {
        tx: ClientTx,
        token: Option<String>,
        nickname: String,
        ip: Option<IpAddr>,
        reply: oneshot::Sender<Result<i32, String>>,
    },
    Answer {
        player_id: i32,
        choice: usize,
    },
    StudentDetach {
        player_id: i32,
        tx: ClientTx,
    },
    Timer {
        generation: u64,
    },
}

#[derive(Clone)]
pub struct RoomHandle {
    pub code: String,
    tx: mpsc::Sender<Cmd>,
}

impl RoomHandle {
    pub fn is_alive(&self) -> bool {
        !self.tx.is_closed()
    }

    pub fn attach_teacher(&self, tx: ClientTx) {
        let _ = self.tx.try_send(Cmd::TeacherAttach(tx));
    }

    pub fn teacher(&self, cmd: TeacherCmd) {
        let _ = self.tx.try_send(Cmd::Teacher(cmd));
    }

    pub fn join_student(
        &self,
        tx: ClientTx,
        token: Option<String>,
        nickname: String,
        ip: Option<IpAddr>,
    ) -> oneshot::Receiver<Result<i32, String>> {
        let (reply, rx) = oneshot::channel();
        let _ = self.tx.try_send(Cmd::StudentJoin {
            tx,
            token,
            nickname,
            ip,
            reply,
        });
        rx
    }

    pub fn answer(&self, player_id: i32, choice: usize) {
        let _ = self.tx.try_send(Cmd::Answer { player_id, choice });
    }

    pub fn detach_student(&self, player_id: i32, tx: ClientTx) {
        let _ = self.tx.try_send(Cmd::StudentDetach { player_id, tx });
    }
}

pub struct RoomSpec {
    pub db: Db,
    pub session_id: i32,
    pub code: String,
    pub seconds: u32,
    pub max_players: usize,
    pub questions: Vec<Question>,
    /// End the session on its own when nothing happens for this long.
    pub idle_timeout: Option<Duration>,
}

struct PlayerState {
    nickname: String,
    token: String,
    points: i64,
    /// Last address seen for this player, used to recover a lost token.
    ip: Option<IpAddr>,
}

struct Room {
    db: Db,
    session_id: i32,
    code: String,
    seconds: u32,
    current_seconds: u32,
    max_players: usize,
    questions: Vec<Question>,
    index: Option<usize>,
    phase: Phase,
    deadline: Option<i64>,
    paused_remaining: Option<i64>,
    order: [usize; 4],
    answers: HashMap<(usize, i32), AnswerState>,
    players: HashMap<i32, PlayerState>,
    current_sq: Option<i32>,
    teacher_tx: Option<ClientTx>,
    student_txs: HashMap<i32, ClientTx>,
    cmd_tx: mpsc::Sender<Cmd>,
    generation: u64,
    idle_timeout: Option<Duration>,
    closed: bool,
}

pub fn spawn(spec: RoomSpec, teacher_tx: ClientTx) -> RoomHandle {
    let (tx, rx) = mpsc::channel(ROOM_QUEUE);
    let handle = RoomHandle {
        code: spec.code.clone(),
        tx: tx.clone(),
    };
    let room = Room {
        db: spec.db,
        session_id: spec.session_id,
        code: spec.code,
        seconds: spec.seconds,
        current_seconds: spec.seconds,
        max_players: spec.max_players,
        questions: spec.questions,
        index: None,
        phase: Phase::Lobby,
        deadline: None,
        paused_remaining: None,
        order: identity_order(),
        answers: HashMap::new(),
        players: HashMap::new(),
        current_sq: None,
        teacher_tx: Some(teacher_tx),
        student_txs: HashMap::new(),
        cmd_tx: tx,
        generation: 0,
        idle_timeout: spec.idle_timeout,
        closed: false,
    };
    tokio::spawn(room.run(rx));
    handle
}

impl Room {
    async fn run(mut self, mut rx: mpsc::Receiver<Cmd>) {
        loop {
            // Re-armed after every command, so the delay is measured from the last
            // activity in the room. `None` disables the guard entirely.
            let idle_timeout = self.idle_timeout;
            let idle = async move {
                match idle_timeout {
                    Some(timeout) => tokio::time::sleep(timeout).await,
                    None => std::future::pending().await,
                }
            };

            tokio::select! {
                message = rx.recv() => {
                    let Some(cmd) = message else { break };
                    match cmd {
                        Cmd::TeacherAttach(tx) => {
                            self.teacher_tx = Some(tx);
                            self.send_room_state_teacher();
                            // Replay whatever the teacher's screen needs after a reload.
                            if let Some(index) = self.index {
                                match self.phase {
                                    Phase::Question => {
                                        let question = self.questions[index].clone();
                                        self.send_question_teacher(&question, index);
                                        if self.paused_remaining.is_some() {
                                            self.send_paused(true, self.paused_remaining);
                                        }
                                    }
                                    Phase::RevealAnswer => {
                                        let question = self.questions[index].clone();
                                        self.send_question_teacher(&question, index);
                                        if let Some(message) = self.reveal_message(index) {
                                            self.send_teacher(message);
                                        }
                                    }
                                    Phase::RevealLeaderboard => {
                                        self.send_teacher(ServerMsg::Leaderboard {
                                            top: self.top(7),
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            self.send_current_answers_status();
                        }
                        Cmd::Teacher(cmd) => self.handle_teacher(cmd).await,
                        Cmd::StudentJoin {
                            tx,
                            token,
                            nickname,
                            ip,
                            reply,
                        } => {
                            let result = self.join_student(tx, token, nickname, ip).await;
                            let _ = reply.send(result);
                        }
                        Cmd::Answer { player_id, choice } => {
                            if let Err(message) = self.answer(player_id, choice).await {
                                if let Some(tx) = self.student_txs.get(&player_id) {
                                    let _ = tx.try_send(ServerMsg::Error { message });
                                }
                            }
                        }
                        Cmd::StudentDetach { player_id, tx } => {
                            // A reload reconnects before the old socket's detach is
                            // handled: only drop the sender this player still uses.
                            if self
                                .student_txs
                                .get(&player_id)
                                .is_some_and(|current| current.same_channel(&tx))
                            {
                                self.student_txs.remove(&player_id);
                            }
                        }
                        Cmd::Timer { generation } => {
                            if generation == self.generation {
                                match self.phase {
                                    Phase::Question => self.reveal(),
                                    Phase::RevealAnswer => self.show_leaderboard(),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                _ = idle => {
                    tracing::info!(
                        session_id = self.session_id,
                        "room idle for too long, ending the session"
                    );
                    self.end().await;
                }
            }
            if self.closed {
                break;
            }
        }
    }

    // ---------------------------------------------------------------- teacher

    async fn handle_teacher(&mut self, cmd: TeacherCmd) {
        match cmd {
            TeacherCmd::StartQuestion | TeacherCmd::Next => {
                if matches!(
                    self.phase,
                    Phase::Lobby | Phase::RevealAnswer | Phase::RevealLeaderboard
                ) {
                    self.start_next().await;
                } else {
                    self.error_teacher("Une question est déjà en cours.");
                }
            }
            TeacherCmd::ForceReveal => {
                if self.phase == Phase::Question {
                    self.reveal();
                }
            }
            TeacherCmd::ShowLeaderboard => self.show_leaderboard(),
            TeacherCmd::Pause => self.pause(),
            TeacherCmd::Resume => self.resume(),
            TeacherCmd::Skip => self.skip().await,
            TeacherCmd::End => self.end().await,
        }
    }

    async fn start_next(&mut self) {
        self.generation += 1;
        let next = self.index.map(|i| i + 1).unwrap_or(0);
        if next >= self.questions.len() {
            self.end().await;
            return;
        }
        self.index = Some(next);
        let question = self.questions[next].clone();
        let order = if question.mixed_choices {
            shuffle_order(&mut rand::thread_rng())
        } else {
            identity_order()
        };
        self.order = order;
        let order_string = order_to_string(&order);

        let sq_id = match db::add_session_question(
            &self.db,
            self.session_id,
            next as i32,
            &question.qid,
            &order_string,
        )
        .await
        {
            Ok(id) => id,
            Err(err) => {
                self.error_teacher(&format!("Impossible d'enregistrer la question : {err}"));
                return;
            }
        };
        let question_seconds = question
            .seconds
            .map(|seconds| seconds as u32)
            .filter(|seconds| *seconds > 0)
            .unwrap_or(self.seconds);
        self.current_seconds = question_seconds;

        self.current_sq = Some(sq_id);
        self.phase = Phase::Question;
        self.deadline = Some(now_ms() + question_seconds as i64 * 1000);
        self.paused_remaining = None;

        self.send_question_all(&question, next);
        self.send_current_answers_status();
        self.send_room_state_all();
        self.schedule(question_seconds as i64 * 1000);
    }

    fn reveal(&mut self) {
        let Some(position) = self.index else { return };
        if self.phase != Phase::Question {
            return;
        }
        self.phase = Phase::RevealAnswer;
        self.deadline = None;
        self.paused_remaining = None;
        self.generation += 1;
        self.send_room_state_all();

        if let Some(message) = self.reveal_message(position) {
            self.send_teacher(message);
        }
        for (player_id, tx) in &self.student_txs {
            let correct = self
                .answers
                .get(&(position, *player_id))
                .map(|a| a.is_correct)
                .unwrap_or(false);
            let _ = tx.try_send(ServerMsg::QuestionResult { correct });
        }
    }

    /// The reveal payload for one question, rebuilt from the current answers.
    fn reveal_message(&self, position: usize) -> Option<ServerMsg> {
        let question = self.questions.get(position)?;
        let correct_index = self
            .order
            .iter()
            .position(|&original| original == question.answer_idx as usize)
            .unwrap_or(0);
        let mut counts = [0usize; 4];
        for ((pos, _), answer) in &self.answers {
            if *pos == position {
                counts[answer.display_choice] += 1;
            }
        }
        Some(ServerMsg::QuestionReveal {
            correct_index,
            counts,
            correct_text: question.choices()[question.answer_idx as usize].to_string(),
        })
    }

    fn show_leaderboard(&mut self) {
        if self.phase != Phase::RevealAnswer {
            return;
        }
        self.phase = Phase::RevealLeaderboard;
        self.generation += 1;
        self.send_room_state_all();
        self.send_teacher(ServerMsg::Leaderboard { top: self.top(7) });
    }

    fn pause(&mut self) {
        if self.phase != Phase::Question || self.paused_remaining.is_some() {
            return;
        }
        let Some(deadline) = self.deadline else {
            return;
        };
        self.paused_remaining = Some((deadline - now_ms()).max(0));
        self.deadline = None;
        self.generation += 1;
        self.send_paused(true, self.paused_remaining);
    }

    fn resume(&mut self) {
        let Some(remaining) = self.paused_remaining.take() else {
            return;
        };
        self.deadline = Some(now_ms() + remaining);
        self.send_paused(false, Some(remaining));
        self.schedule(remaining);
    }

    async fn skip(&mut self) {
        if matches!(self.phase, Phase::Lobby | Phase::Podium) {
            return;
        }
        self.generation += 1;
        if let Some(sq_id) = self.current_sq.take() {
            if let Err(err) = db::remove_session_question(&self.db, sq_id).await {
                self.error_teacher(&format!("Impossible de supprimer la question : {err}"));
            }
        }
        if let Some(position) = self.index {
            self.answers.retain(|(pos, _), _| *pos != position);
        }
        self.phase = Phase::RevealLeaderboard;
        self.start_next().await;
    }

    async fn end(&mut self) {
        if self.phase == Phase::Podium {
            return;
        }
        self.phase = Phase::Podium;
        self.generation += 1;
        let ranking = self.ranking();
        let podium: Vec<ScoreView> = ranking.iter().take(3).cloned().collect();
        let top: Vec<ScoreView> = ranking.iter().take(7).cloned().collect();

        self.send_teacher(ServerMsg::SessionEnd {
            podium: podium.clone(),
            top: top.clone(),
            my_rank: None,
            my_points: None,
        });
        for (player_id, tx) in &self.student_txs {
            let me = &self.players[player_id];
            let rank = ranking
                .iter()
                .position(|s| s.nickname == me.nickname)
                .map(|i| i + 1);
            let _ = tx.try_send(ServerMsg::SessionEnd {
                podium: podium.clone(),
                top: top.clone(),
                my_rank: rank,
                my_points: Some(me.points),
            });
        }
        let _ = db::finish_session(&self.db, self.session_id, now_ms()).await;
        self.closed = true;
    }

    // ---------------------------------------------------------------- student

    async fn join_student(
        &mut self,
        tx: ClientTx,
        token: Option<String>,
        nickname: String,
        ip: Option<IpAddr>,
    ) -> Result<i32, String> {
        if self.phase == Phase::Podium {
            return Err("La partie est terminée.".into());
        }

        if let Some(existing_id) = token.as_deref().and_then(|t| self.by_token(t)) {
            return Ok(self.reattach(existing_id, tx, ip));
        }

        // No usable token: the browser may have lost it (private mode, storage
        // cleared). Recover the player when the same address reuses its nickname.
        if let Some(existing_id) = self.by_ip_and_nickname(ip, &nickname) {
            return Ok(self.reattach(existing_id, tx, ip));
        }

        if self.players.len() >= self.max_players {
            return Err("La partie est complète.".into());
        }

        let nickname = sanitize_nickname(&nickname);
        let nickname = self.unique_nickname(&nickname);
        let token = new_token();
        let id = db::add_player(&self.db, self.session_id, &token, &nickname, now_ms())
            .await
            .map_err(|e| format!("Impossible de rejoindre : {e}"))?;

        self.players.insert(
            id,
            PlayerState {
                nickname,
                token: token.clone(),
                points: 0,
                ip,
            },
        );
        self.student_txs.insert(id, tx.clone());

        let _ = tx.try_send(ServerMsg::PlayerWelcome {
            token,
            player_id: id,
        });
        let _ = tx.try_send(ServerMsg::RoomState {
            phase: self.phase,
            code: self.code.clone(),
            player_count: self.players.len(),
            players: self.player_views(),
        });
        if self.phase == Phase::Question {
            if let Some(index) = self.index {
                let question = self.questions[index].clone();
                self.send_question_to(&tx, &question, index);
            }
        }
        self.send_room_state_teacher();
        self.send_current_answers_status();
        Ok(id)
    }

    /// Re-attach a socket to an existing player and replay its current view.
    fn reattach(&mut self, existing_id: i32, tx: ClientTx, ip: Option<IpAddr>) -> i32 {
        if let Some(player) = self.players.get_mut(&existing_id) {
            if ip.is_some() {
                player.ip = ip;
            }
        }
        self.student_txs.insert(existing_id, tx.clone());
        let me = &self.players[&existing_id];
        let _ = tx.try_send(ServerMsg::PlayerWelcome {
            token: me.token.clone(),
            player_id: existing_id,
        });
        let _ = tx.try_send(ServerMsg::RoomState {
            phase: self.phase,
            code: self.code.clone(),
            player_count: self.players.len(),
            players: self.player_views(),
        });
        if let Some(index) = self.index {
            let question = self.questions[index].clone();
            self.send_question_to(&tx, &question, index);
            // Restore the player's own answer so a reload shows the same state.
            if let Some(answer) = self.answers.get(&(index, existing_id)) {
                let _ = tx.try_send(ServerMsg::AnswerSaved {
                    choice: answer.display_choice,
                });
            }
            match self.phase {
                Phase::Question => {
                    if let Some(remaining) = self.paused_remaining {
                        let _ = tx.try_send(ServerMsg::Paused {
                            paused: true,
                            remaining_ms: Some(remaining),
                        });
                    }
                }
                Phase::RevealAnswer | Phase::RevealLeaderboard => {
                    let correct = self
                        .answers
                        .get(&(index, existing_id))
                        .map(|answer| answer.is_correct)
                        .unwrap_or(false);
                    let _ = tx.try_send(ServerMsg::QuestionResult { correct });
                }
                _ => {}
            }
        }
        self.send_room_state_teacher();
        self.send_current_answers_status();
        existing_id
    }

    fn by_token(&self, token: &str) -> Option<i32> {
        self.players
            .iter()
            .find(|(_, p)| p.token == token)
            .map(|(id, _)| *id)
    }

    /// Players are unique per room and nicknames are unique too, so the same
    /// address + nickname identifies a player who lost their token.
    fn by_ip_and_nickname(&self, ip: Option<IpAddr>, nickname: &str) -> Option<i32> {
        let ip = ip?;
        let wanted = sanitize_nickname(nickname).to_lowercase();
        self.players
            .iter()
            .find(|(_, player)| player.ip == Some(ip) && player.nickname.to_lowercase() == wanted)
            .map(|(id, _)| *id)
    }

    fn unique_nickname(&self, wanted: &str) -> String {
        let exists = |name: &str| self.players.values().any(|p| p.nickname == name);
        if !exists(wanted) {
            return wanted.to_string();
        }
        for suffix in 2..1000 {
            let candidate = format!("{wanted} ({suffix})");
            if !exists(&candidate) {
                return candidate;
            }
        }
        wanted.to_string()
    }

    async fn answer(&mut self, player_id: i32, choice: usize) -> Result<(), String> {
        if self.phase != Phase::Question {
            return Err("La question n'est pas ouverte.".into());
        }
        let now = now_ms();
        // While paused the deadline is frozen: answers stay open and are scored from
        // the remaining time at the moment of the pause.
        let remaining = match (self.deadline, self.paused_remaining) {
            (Some(deadline), _) => {
                if now > deadline {
                    return Err("Le temps est écoulé.".into());
                }
                (deadline - now).max(0)
            }
            (None, Some(paused)) => paused,
            (None, None) => return Err("Aucune question en cours.".into()),
        };
        if choice > 3 {
            return Err("Choix invalide.".into());
        }
        if !self.players.contains_key(&player_id) {
            return Err("Joueur inconnu.".into());
        }
        let Some(position) = self.index else {
            return Err("Aucune question en cours.".into());
        };
        let Some(session_question_id) = self.current_sq else {
            return Err("Aucune question en cours.".into());
        };

        let question = self.questions[position].clone();
        let original_choice = self.order[choice];
        let is_correct = original_choice == question.answer_idx as usize;
        let limit_ms = self.current_seconds as i64 * 1000;
        let points = bia_core::score::award(is_correct, remaining, limit_ms) as i32;
        let ms_elapsed = (limit_ms - remaining) as i32;

        db::upsert_answer(
            &self.db,
            NewAnswer {
                session_question_id,
                player_id,
                choice_idx: original_choice as i32,
                is_correct,
                points,
                answered_at: now,
                ms_elapsed,
            },
        )
        .await
        .map_err(|e| format!("Erreur base de données : {e}"))?;

        let previous = self
            .answers
            .get(&(position, player_id))
            .map(|a| a.points)
            .unwrap_or(0);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.points += (points - previous) as i64;
        }
        self.answers.insert(
            (position, player_id),
            AnswerState {
                display_choice: choice,
                original_choice,
                is_correct,
                points,
                answered_at: now,
                ms_elapsed,
            },
        );

        if let Some(tx) = self.student_txs.get(&player_id) {
            let _ = tx.try_send(ServerMsg::AnswerSaved { choice });
        }
        self.send_current_answers_status();
        Ok(())
    }

    // ---------------------------------------------------------------- helpers

    fn ranking(&self) -> Vec<ScoreView> {
        let mut ranking: Vec<ScoreView> = self
            .players
            .values()
            .map(|p| ScoreView {
                nickname: p.nickname.clone(),
                points: p.points,
            })
            .collect();
        ranking.sort_by(|a, b| b.points.cmp(&a.points).then(a.nickname.cmp(&b.nickname)));
        ranking
    }

    fn top(&self, n: usize) -> Vec<ScoreView> {
        self.ranking().into_iter().take(n).collect()
    }

    fn choices(&self, question: &Question) -> Vec<ChoiceView> {
        let originals = question.choices();
        (0..4)
            .map(|position| ChoiceView::at(originals[self.order[position]], position))
            .collect()
    }

    fn question_start(&self, question: &Question, index: usize) -> ServerMsg {
        ServerMsg::QuestionStart {
            index,
            total: self.questions.len(),
            text: question.text.clone(),
            figure_url: question.figure_url(),
            choices: self.choices(question),
            ends_at: self.deadline.unwrap_or_else(now_ms),
            server_now: now_ms(),
            seconds: self.current_seconds,
        }
    }

    fn send_question_all(&self, question: &Question, index: usize) {
        let message = self.question_start(question, index);
        self.send_teacher(message.clone());
        for tx in self.student_txs.values() {
            let _ = tx.try_send(message.clone());
        }
    }

    fn send_question_teacher(&self, question: &Question, index: usize) {
        self.send_teacher(self.question_start(question, index));
    }

    fn send_question_to(&self, tx: &ClientTx, question: &Question, index: usize) {
        let _ = tx.try_send(self.question_start(question, index));
    }

    fn send_teacher(&self, message: ServerMsg) {
        if let Some(tx) = &self.teacher_tx {
            let _ = tx.try_send(message);
        }
    }

    fn error_teacher(&self, message: &str) {
        self.send_teacher(ServerMsg::Error {
            message: message.to_string(),
        });
    }

    fn send_paused(&self, paused: bool, remaining_ms: Option<i64>) {
        self.send_teacher(ServerMsg::Paused {
            paused,
            remaining_ms,
        });
        for tx in self.student_txs.values() {
            let _ = tx.try_send(ServerMsg::Paused {
                paused,
                remaining_ms,
            });
        }
    }

    fn player_views(&self) -> Vec<PlayerView> {
        self.players
            .values()
            .map(|p| PlayerView {
                nickname: p.nickname.clone(),
                points: p.points,
            })
            .collect()
    }

    fn send_room_state_teacher(&self) {
        if let Some(tx) = &self.teacher_tx {
            let _ = tx.try_send(ServerMsg::RoomState {
                phase: self.phase,
                code: self.code.clone(),
                player_count: self.players.len(),
                players: self.player_views(),
            });
        }
    }

    fn send_room_state_all(&self) {
        self.send_room_state_teacher();
        for tx in self.student_txs.values() {
            let _ = tx.try_send(ServerMsg::RoomState {
                phase: self.phase,
                code: self.code.clone(),
                player_count: self.players.len(),
                players: self.player_views(),
            });
        }
    }

    fn answer_views(&self, position: usize) -> Vec<PlayerAnswerView> {
        let mut players: Vec<PlayerAnswerView> = self
            .players
            .iter()
            .map(|(id, player)| PlayerAnswerView {
                nickname: player.nickname.clone(),
                answered: self.answers.contains_key(&(position, *id)),
            })
            .collect();
        players.sort_by(|a, b| a.nickname.cmp(&b.nickname));
        players
    }

    fn send_answers_status(&self, position: usize) {
        let players = self.answer_views(position);
        self.send_teacher(ServerMsg::AnswersCount {
            count: players.iter().filter(|player| player.answered).count(),
            total: players.len(),
            players,
        });
    }

    fn send_current_answers_status(&self) {
        if matches!(self.phase, Phase::Question | Phase::RevealAnswer) {
            if let Some(position) = self.index {
                self.send_answers_status(position);
            }
        }
    }

    fn schedule(&mut self, delay_ms: i64) {
        self.generation += 1;
        let generation = self.generation;
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms.max(0) as u64)).await;
            let _ = tx.send(Cmd::Timer { generation }).await;
        });
    }
}

fn sanitize_nickname(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control() && !is_invisible(*c))
        .take(24)
        .collect();
    if cleaned.is_empty() {
        "Joueur".to_string()
    } else {
        cleaned
    }
}

/// Zero-width and bidi-override characters used to spoof or hide names.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

fn new_token() -> String {
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
