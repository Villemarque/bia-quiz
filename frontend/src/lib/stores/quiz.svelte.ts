import { JsonSocket } from '$lib/ws';
import { writeLocal } from '$lib/env';
import type { ChapterPick, Phase, QuestionStart, Reveal, Score, SessionEnd } from '$lib/types';

type Role = 'teacher' | 'student';

/**
 * One client for both roles. The room is the source of truth: this object only
 * mirrors what the server pushes, plus a local countdown for display.
 */
export class QuizClient {
  readonly role: Role;

  connected = $state(false);
  roomSeen = $state(false);
  phase = $state<Phase>('lobby');
  code = $state('');
  joinUrl = $state('');
  available = $state(0);
  requested = $state(0);
  warning = $state('');
  players = $state<Score[]>([]);
  playerCount = $state(0);
  question = $state<QuestionStart | null>(null);
  reveal = $state<Reveal | null>(null);
  leaderboard = $state<Score[]>([]);
  paused = $state(false);
  final = $state<SessionEnd | null>(null);
  answer = $state<number | null>(null);
  result = $state<boolean | null>(null);
  answeredCount = $state(0);
  error = $state('');
  remaining = $state(0);
  token = $state('');
  playerId = $state<number | null>(null);

  private socket: JsonSocket | null = null;
  private joinArgs: { token: string; nickname: string } | null = null;
  private deadline = 0;
  private ticker: number | null = null;

  constructor(role: Role) {
    this.role = role;
  }

  connectTeacher(room?: string): void {
    this.code = room ?? '';
    const params = new URLSearchParams({ role: 'teacher' });
    if (room) params.set('room', room);
    this.open(`/ws?${params.toString()}`);
  }

  connectStudent(code: string, token: string, nickname: string): void {
    this.code = code;
    this.joinArgs = { token, nickname };
    const params = new URLSearchParams({ role: 'student', room: code });
    if (token) params.set('token', token);
    this.open(`/ws?${params.toString()}`);
  }

  private open(path: string): void {
    this.socket?.close();
    this.socket = new JsonSocket(
      path,
      (message) => this.handle(message),
      () => this.onOpen(),
    );
  }

  private onOpen(): void {
    this.connected = true;
    if (this.role === 'student' && this.joinArgs) {
      this.socket?.send({
        type: 'student.join',
        nickname: this.joinArgs.nickname,
      });
    }
  }

  private handle(message: any): void {
    switch (message.type) {
      case 'hello':
        this.connected = true;
        break;
      case 'room.created':
        this.roomSeen = true;
        this.code = message.code;
        this.joinUrl = message.join_url;
        this.available = message.available;
        this.requested = message.requested;
        this.warning = message.warning ?? '';
        break;
      case 'room.state':
        this.roomSeen = true;
        this.phase = message.phase;
        if (message.code) this.code = message.code;
        this.playerCount = message.player_count ?? 0;
        if (Array.isArray(message.players) && message.players.length > 0) {
          this.players = message.players;
        }
        break;
      case 'player.welcome':
        this.token = message.token;
        this.playerId = message.player_id;
        writeLocal('bia-token', message.token);
        break;
      case 'question.start':
        this.question = message as QuestionStart;
        this.phase = 'question';
        this.reveal = null;
        this.result = null;
        this.answer = null;
        this.answeredCount = 0;
        this.leaderboard = [];
        this.paused = false;
        this.startTimer(message.ends_at - message.server_now);
        break;
      case 'answers.count':
        this.answeredCount = message.count;
        break;
      case 'answer.saved':
        this.answer = message.choice;
        break;
      case 'question.reveal':
        this.reveal = message as Reveal;
        this.phase = 'reveal_answer';
        this.stopTimer();
        this.remaining = 0;
        break;
      case 'question.result':
        this.result = Boolean(message.correct);
        break;
      case 'leaderboard':
        this.leaderboard = message.top;
        this.phase = 'reveal_leaderboard';
        break;
      case 'paused':
        this.paused = Boolean(message.paused);
        if (this.paused) {
          this.stopTimer();
        } else if (typeof message.remaining_ms === 'number') {
          this.startTimer(message.remaining_ms);
        }
        break;
      case 'session.end':
        this.final = message as SessionEnd;
        this.phase = 'podium';
        this.stopTimer();
        this.remaining = 0;
        break;
      case 'error':
        this.error = message.message;
        break;
    }
  }

  private startTimer(milliseconds: number): void {
    this.stopTimer();
    this.remaining = Math.max(0, milliseconds);
    this.deadline = performance.now() + this.remaining;
    this.ticker = window.setInterval(() => {
      this.remaining = Math.max(0, this.deadline - performance.now());
      if (this.remaining <= 0) this.stopTimer();
    }, 100);
  }

  private stopTimer(): void {
    if (this.ticker !== null) {
      window.clearInterval(this.ticker);
      this.ticker = null;
    }
  }

  create(chapters: ChapterPick[], seconds: number): void {
    this.error = '';
    this.socket?.send({ type: 'teacher.create', chapters, seconds });
  }

  startQuestion(): void {
    this.error = '';
    this.socket?.send({ type: 'teacher.start_question' });
  }

  forceReveal(): void {
    this.socket?.send({ type: 'teacher.force_reveal' });
  }

  next(): void {
    this.error = '';
    this.socket?.send({ type: 'teacher.next' });
  }

  pause(): void {
    this.socket?.send({ type: 'teacher.pause' });
  }

  resume(): void {
    this.socket?.send({ type: 'teacher.resume' });
  }

  skip(): void {
    this.socket?.send({ type: 'teacher.skip' });
  }

  end(): void {
    this.socket?.send({ type: 'teacher.end' });
  }

  choose(choice: number): void {
    if (this.phase !== 'question' || this.remaining <= 0) return;
    this.socket?.send({ type: 'student.answer', choice });
  }

  destroy(): void {
    this.stopTimer();
    this.socket?.close();
  }
}
