import type { ReactNode } from "react";
import type { DocLine } from "../lib/doc";
import { mdLine, type MdLine, type Span } from "../lib/markdown";
import type { Severity } from "../types";
import type { FindingItem } from "./model";

const RANK: Record<Severity, number> = { critical: 0, major: 1, minor: 2 };

interface Seg {
  text: string;
  hits: FindingItem[];
  strong: boolean;
  code: boolean;
}

const inside = (spans: Span[], base: number, a: number, b: number) => spans.some(([s, e]) => base + s <= a && base + e >= b);

// Cuts [from, to) of the line at every finding and markup boundary; markup characters are dropped.
function segments(line: DocLine, md: MdLine, from: number, to: number, hits: FindingItem[]): Seg[] {
  const points = new Set([from, to]);
  const add = (p: number) => p > from && p < to && points.add(p);
  for (const h of hits) if (h.range) [h.range.start, h.range.end].forEach(add);
  for (const [s0, e0] of [...md.hidden, ...md.strong, ...md.code]) [line.start + s0, line.start + e0].forEach(add);
  const sorted = [...points].sort((a, b) => a - b);
  const out: Seg[] = [];
  for (let k = 0; k + 1 < sorted.length; k++) {
    const a = sorted[k] as number;
    const b = sorted[k + 1] as number;
    if (a === b || inside(md.hidden, line.start, a, b)) continue;
    out.push({
      text: line.text.slice(a - line.start, b - line.start),
      hits: hits.filter((h) => h.range && h.range.start <= a && h.range.end >= b),
      strong: inside(md.strong, line.start, a, b),
      code: inside(md.code, line.start, a, b),
    });
  }
  return out;
}

export function hitsByLine(lines: DocLine[], items: FindingItem[]): Map<number, FindingItem[]> {
  const map = new Map<number, FindingItem[]>();
  for (const it of items) {
    if (!it.range) continue;
    const { start, end } = it.range;
    for (const l of lines) {
      if (l.end < start || l.start >= end) continue;
      map.set(l.global, [...(map.get(l.global) ?? []), it]);
    }
  }
  return map;
}

interface Props {
  lines: DocLine[];
  hits: Map<number, FindingItem[]>;
  selected: number | null;
  onSelect: (i: number) => void;
  margin?: (line: DocLine) => ReactNode;
  wideMargin?: boolean;
  marked?: Set<number>; // lines that carry a margin note
}

export function DocView({ lines, hits, selected, onSelect, margin, wideMargin, marked }: Props) {
  const multiPage = lines.some((l) => l.page > 1);
  return (
    <div className={`doc ${margin ? (wideMargin ? "with-wide-margin" : "with-margin") : ""}`}>
      {lines.map((line, idx) => {
        const lineHits = hits.get(line.global) ?? [];
        const md = mdLine(line.text);
        if (md.kind === "sep") return null;
        const next = lines[idx + 1];
        const header = md.kind === "row" && next !== undefined && next.page === line.page && mdLine(next.text).kind === "sep";
        const paint = (from: number, to: number) =>
          segments(line, md, from, to, lineHits).map((sg, k) => {
            const body = sg.strong ? <strong>{sg.text}</strong> : sg.code ? <code>{sg.text}</code> : sg.text;
            if (sg.hits.length === 0) return <span key={k}>{body}</span>;
            const top = [...sg.hits].sort((a, b) => RANK[a.f.severity] - RANK[b.f.severity])[0] as FindingItem;
            const sel = sg.hits.some((h) => h.i === selected);
            return (
              <mark
                key={k}
                data-f={top.i}
                tabIndex={0}
                className={`hl hl-${top.closed ? "done" : top.f.severity} ${sel ? "hl-sel" : ""}`}
                onClick={() => onSelect(top.i)}
                onKeyDown={(e) => e.key === "Enter" && onSelect(top.i)}
              >
                {body}
              </mark>
            );
          });
        return (
          <div className="dl-group" key={line.global}>
            {multiPage && line.no === 1 && <div className="doc-page">Страница {line.page}</div>}
            <div className={`dl ${marked?.has(line.global) ? "dl-marked" : ""}`} id={`L${line.global}`}>
              <span className="dl-no mono">{line.global}</span>
              <div className={`dl-text dl-${md.kind} ${md.kind === "h" ? `dl-h${Math.min(md.level, 3)}` : ""} ${header ? "dl-head" : ""} ${line.text.trim() === "" || md.kind === "rule" ? "dl-empty" : ""}`}>
                {md.kind === "row" ? (
                  <div className="dl-row" style={{ gridTemplateColumns: `repeat(${md.cells.length}, minmax(0, 1fr))` }}>
                    {md.cells.map(([a, b], k) => (
                      <span className="dl-cell" key={k}>
                        {paint(line.start + a, line.start + b)}
                      </span>
                    ))}
                  </div>
                ) : line.text.trim() === "" || md.kind === "rule" ? (
                  " "
                ) : (
                  paint(line.start + md.body[0], line.start + md.body[1])
                )}
              </div>
              {margin && <div className="dl-margin">{margin(line)}</div>}
            </div>
          </div>
        );
      })}
    </div>
  );
}
