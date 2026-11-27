import type { Finding } from "../types";
import { parseAnchor } from "./anchors";

export interface SectionGroup {
  heading: string;
  findings: Finding[];
  firstPage: number | null;
}

const WHOLE = "Весь документ";

function order(f: Finding): number {
  const a = parseAnchor(f.section);
  return a.line === null ? Number.MAX_SAFE_INTEGER : (a.page ?? 0) * 1e6 + a.line;
}

// Groups by the heading part of the anchor, in document order.
export function groupBySection(findings: Finding[]): SectionGroup[] {
  const groups = new Map<string, SectionGroup & { at: number }>();
  for (const f of findings) {
    const a = parseAnchor(f.section);
    const heading = a.heading ?? WHOLE;
    const g = groups.get(heading) ?? { heading, findings: [], firstPage: a.page, at: Number.MAX_SAFE_INTEGER };
    g.findings.push(f);
    g.at = Math.min(g.at, order(f));
    groups.set(heading, g);
  }
  return [...groups.values()].sort((a, b) => a.at - b.at);
}
