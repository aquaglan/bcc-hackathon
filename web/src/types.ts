// Mirrors the HTTP contract of the server; keep in sync with the API document.
export type Severity = "critical" | "major" | "minor";
export type ReviewStatus = "running" | "awaiting_answers" | "done" | "failed";

export interface Spec {
  title: string | null;
  file_name: string | null;
  text: string;
}
export interface Question {
  id: string;
  text: string;
  reason: string;
  section: string | null;
}
export interface Answer {
  question_id: string;
  text: string;
}
export interface Round {
  questions: Question[];
  answers: Answer[];
}
export interface Requirement {
  id: string;
  source_id: string | null;
  section: string;
  quote: string;
  summary: string;
}
export interface Finding {
  requirement_id: string | null;
  severity: Severity;
  category: string;
  section: string | null;
  quote: string | null;
  problem: string;
  suggestion: string;
  verified: boolean;
  // Absent in older reviews; normalized to null on load.
  critic_note: string | null;
}
export interface Report {
  summary: string;
  requirements: Requirement[];
  findings: Finding[];
  open_questions: Question[];
  models: string[];
  // Absent in older reviews.
  dropped?: Dropped[];
}
export interface Dropped {
  finding: Finding;
  reason: "duplicate" | "refuted";
  kept_in: string | null;
  note: string | null;
}
export interface Review {
  id: string;
  harness: string;
  spec: Spec;
  status: ReviewStatus;
  rounds: Round[];
  report: Report | null;
  error: string | null;
}
export interface ReviewSummary {
  id: string;
  title: string | null;
  file_name: string | null;
  status: ReviewStatus;
  findings: number | null;
  critical: number | null;
  major: number | null;
}
export type Stage = "extract" | "check" | "critic" | "questions" | "done";
export interface ProgressEvent {
  kind: "stage" | "found" | "dropped";
  severity: Severity | null;
  text: string;
  at_ms: number;
}
export interface Progress {
  stage: Stage;
  requirements: number;
  checked: number;
  critical: number;
  major: number;
  minor: number;
  events: ProgressEvent[];
}
