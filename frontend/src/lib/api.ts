import type { SubjectCatalog } from '$lib/types';

function redirectToLogin(): void {
  const next = encodeURIComponent(location.pathname + location.search);
  location.assign(`/login?next=${next}`);
}

async function json<T>(response: Response): Promise<T> {
  if (response.status === 401) {
    redirectToLogin();
    throw new Error('unauthorized');
  }
  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new Error(text || `HTTP ${response.status}`);
  }
  return (await response.json()) as T;
}

export async function fetchCatalog(): Promise<SubjectCatalog[]> {
  return json<SubjectCatalog[]>(await fetch('/api/chapters'));
}

export interface Meta {
  source_url: string;
  join_code_length: number;
  version: string;
  commit: string;
  commit_url: string | null;
}

export async function fetchMeta(): Promise<Meta> {
  return json<Meta>(await fetch('/api/meta'));
}

export interface RoomStatus {
  live: boolean;
  code: string | null;
  join_url: string | null;
}

export async function fetchRoom(): Promise<RoomStatus> {
  return json<RoomStatus>(await fetch('/api/room'));
}

export interface Me {
  username: string;
  role: 'prof' | 'admin';
}

export async function fetchMe(): Promise<Me> {
  return json<Me>(await fetch('/api/auth/me'));
}

export interface SessionListRow {
  id: number;
  code: string;
  started_at: number;
  ended_at: number | null;
  question_count: number;
  seconds_per_question: number;
  players: number;
}

export interface SessionQuestionRow {
  position: number;
  qid: string;
  text: string;
  display_order: string;
  answer_idx: number;
}

export interface SessionAnswerRow {
  nickname: string;
  position: number;
  choice_idx: number;
  is_correct: boolean;
  points: number;
}

export interface SessionDetail {
  session: {
    id: number;
    code: string;
    started_at: number;
    ended_at: number | null;
    question_count: number;
    seconds_per_question: number;
  };
  questions: SessionQuestionRow[];
  answers: SessionAnswerRow[];
}

export async function fetchSessions(): Promise<SessionListRow[]> {
  return json<SessionListRow[]>(await fetch('/api/prof/sessions'));
}

export async function fetchSession(id: number): Promise<SessionDetail> {
  return json<SessionDetail>(await fetch(`/api/prof/sessions/${id}`));
}

// ---------------------------------------------------------------- admin

export interface AdminStats {
  questions: number;
  active: number;
  figures: number;
  chapters: number;
  db_bytes: number;
  upstream_url: string;
}

export interface SkippedRow {
  record: number;
  line: number;
  qid: string;
  reason: string;
}

export interface ImportReport {
  read: number;
  imported: number;
  figures: number;
  skipped: SkippedRow[];
  reasons: Record<string, number>;
  per_chapter: Record<string, number>;
}

export interface ImportJob {
  state: 'idle' | 'downloading' | 'importing' | 'done' | 'failed';
  done: number;
  report: ImportReport | null;
  error: string | null;
  source: string | null;
  started_at: number | null;
  finished_at: number | null;
}

export async function fetchAdminStats(): Promise<AdminStats> {
  return json<AdminStats>(await fetch('/api/admin/stats'));
}

export async function fetchJob(): Promise<ImportJob> {
  return json<ImportJob>(await fetch('/api/admin/import'));
}

export async function startImport(): Promise<void> {
  const response = await fetch('/api/admin/import', { method: 'POST' });
  if (response.status === 401) {
    redirectToLogin();
    return;
  }
  if (!response.ok) {
    throw new Error((await response.text()) || `HTTP ${response.status}`);
  }
}
