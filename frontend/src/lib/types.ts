export type Phase =
  | 'lobby'
  | 'question'
  | 'reveal_answer'
  | 'reveal_leaderboard'
  | 'podium';

export interface Choice {
  text: string;
  shape: 'triangle' | 'diamond' | 'circle' | 'square' | string;
  letter: string;
}

export interface Score {
  nickname: string;
  points: number;
}

export interface QuestionStart {
  type: 'question.start';
  index: number;
  total: number;
  text: string;
  figure_url: string | null;
  choices: Choice[];
  ends_at: number;
  server_now: number;
  seconds: number;
}

export interface Reveal {
  type: 'question.reveal';
  correct_index: number;
  counts: number[];
  correct_text: string;
}

export interface SessionEnd {
  type: 'session.end';
  podium: Score[];
  top: Score[];
  my_rank: number | null;
  my_points: number | null;
}

export interface Chapter {
  number: number;
  title: string;
  question_count: number;
}

export interface SubjectCatalog {
  subject: number;
  title: string;
  question_count: number;
  chapters: Chapter[];
  no_chapter_count: number;
}

export interface ChapterPick {
  subject: number;
  chapter: number | null;
  count: number;
}
