import type { Finding, Severity } from "../types";
import { load, save } from "./store";

// What the reader did with a finding. "edited" means they wrote their own fix instead of the agent's.
export type Status = "open" | "accepted" | "edited" | "rejected";

export interface Decision {
  status: Status;
  fix: string | null;
  comment: string | null;
}

export const OPEN: Decision = { status: "open", fix: null, comment: null };

export const STATUS_LABEL: Record<Status, string> = {
  open: "Открыто",
  accepted: "Принято",
  edited: "Своя правка",
  rejected: "Отклонено",
};

// A closed finding no longer counts against readiness: it is fixed or dismissed.
export const isClosed = (d: Decision) => d.status !== "open";

// shortcut: decisions live in this browser only, move them to the server once several people review one document.
export function loadDecisions(reviewId: string): Record<string, Decision> {
  const stored = load<Record<string, Decision> | null>(`anyq:decisions:${reviewId}`, null);
  if (stored) return stored;
  // Before decisions there was only a list of accepted keys.
  const accepted = load<string[]>(`anyq:accepted:${reviewId}`, []);
  return Object.fromEntries(accepted.map((k) => [k, { ...OPEN, status: "accepted" as const }]));
}

export function saveDecisions(reviewId: string, all: Record<string, Decision>): void {
  save(`anyq:decisions:${reviewId}`, all);
}

export interface Row {
  i: number;
  f: Finding;
  decision: Decision;
  at: number | null; // offset in the document, for document order
}

export type StatusFilter = "all" | "open" | "closed" | Status;
export type Verified = "all" | "yes" | "no";
export type Sort = "severity" | "document" | "category" | "status";

export interface Filter {
  q: string;
  severities: Severity[]; // empty means all
  status: StatusFilter;
  category: string; // "" means all
  verified: Verified;
}

export const NO_FILTER: Filter = { q: "", severities: [], status: "all", category: "", verified: "all" };

const norm = (s: string) => s.toLowerCase().replace(/ё/g, "е");

export function matches(r: Row, f: Filter): boolean {
  if (f.severities.length > 0 && !f.severities.includes(r.f.severity)) return false;
  if (f.category && r.f.category !== f.category) return false;
  if (f.verified !== "all" && r.f.verified !== (f.verified === "yes")) return false;
  const st = r.decision.status;
  if (f.status === "open" && st !== "open") return false;
  if (f.status === "closed" && st === "open") return false;
  if (f.status !== "all" && f.status !== "open" && f.status !== "closed" && st !== f.status) return false;
  const q = norm(f.q.trim());
  if (!q) return true;
  const hay = [r.f.problem, r.f.quote, r.f.suggestion, r.f.section, r.f.category, r.f.requirement_id, r.decision.fix, r.decision.comment];
  return hay.some((t) => t !== null && norm(t).includes(q));
}

const SEV_RANK: Record<Severity, number> = { critical: 0, major: 1, minor: 2 };
const STATUS_RANK: Record<Status, number> = { open: 0, edited: 1, accepted: 2, rejected: 3 };

export function sortRows<T extends Row>(rows: T[], by: Sort): T[] {
  const doc = (r: Row) => r.at ?? Number.MAX_SAFE_INTEGER;
  const key: Record<Sort, (a: Row, b: Row) => number> = {
    severity: (a, b) => SEV_RANK[a.f.severity] - SEV_RANK[b.f.severity] || doc(a) - doc(b),
    document: (a, b) => doc(a) - doc(b) || SEV_RANK[a.f.severity] - SEV_RANK[b.f.severity],
    category: (a, b) => a.f.category.localeCompare(b.f.category) || SEV_RANK[a.f.severity] - SEV_RANK[b.f.severity],
    status: (a, b) => STATUS_RANK[a.decision.status] - STATUS_RANK[b.decision.status] || SEV_RANK[a.f.severity] - SEV_RANK[b.f.severity],
  };
  return [...rows].sort((a, b) => key[by](a, b) || a.i - b.i);
}
