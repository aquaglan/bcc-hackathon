import type { Row } from "../lib/decisions";
import type { DocLine, Range } from "../lib/doc";
import type { Finding, Question } from "../types";

export interface FindingItem extends Row {
  // i: position in report.findings; stable within one loaded review
  key: string; // survives a re-check, used to persist decisions
  range: Range | null;
  line: DocLine | null;
  closed: boolean;
}

export interface QuestionItem {
  n: number; // 1-based position in report.open_questions
  q: Question;
  line: DocLine | null;
}

export function findingKey(f: Finding): string {
  return `${f.category}|${f.requirement_id ?? ""}|${(f.quote ?? f.section ?? f.problem).slice(0, 60)}`;
}
