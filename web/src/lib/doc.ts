import { parseAnchor } from "./anchors";

export interface DocLine {
  page: number; // 1-based; pages are split by form feed
  no: number; // 1-based within the page
  global: number; // 1-based within the whole text
  start: number; // offset in the text
  end: number;
  text: string;
}

export function buildDoc(text: string): DocLine[] {
  const lines: DocLine[] = [];
  let offset = 0;
  text.split("\f").forEach((pageText, p) => {
    pageText.split("\n").forEach((t, i) => {
      lines.push({ page: p + 1, no: i + 1, global: lines.length + 1, start: offset, end: offset + t.length, text: t });
      offset += t.length + 1;
    });
  });
  return lines;
}

export function lineForAnchor(lines: DocLine[], section: string | null): DocLine | null {
  const a = parseAnchor(section);
  if (a.line === null) return null;
  const page = a.page;
  if (page !== null) return lines.find((l) => l.page === page && l.no === a.line) ?? null;
  return lines[a.line - 1] ?? null;
}

export function lineAtOffset(lines: DocLine[], offset: number): DocLine | null {
  let lo = 0;
  let hi = lines.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const l = lines[mid];
    if (!l) return null;
    if (offset < l.start) hi = mid - 1;
    else if (offset > l.end) lo = mid + 1;
    else return l;
  }
  return null;
}

export interface TextIndex {
  norm: string;
  map: number[]; // normalized index -> original index
}

const QUOTES = /[«»"“”„‟]/;
const DASHES = /[–—−]/;

function normChar(c: string): string {
  if (QUOTES.test(c)) return '"';
  if (DASHES.test(c)) return "-";
  if (c === "’" || c === "‘") return "'";
  if (c === "ё" || c === "Ё") return "е";
  return c.toLowerCase();
}

// Whitespace and typographic quotes differ between the model's quote and the
// source text, so both sides are compared in a normalized form.
export function indexText(text: string): TextIndex {
  let norm = "";
  const map: number[] = [];
  let prevSpace = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i] as string;
    if (/\s/.test(c)) {
      if (!prevSpace) {
        norm += " ";
        map.push(i);
      }
      prevSpace = true;
    } else {
      norm += normChar(c);
      map.push(i);
      prevSpace = false;
    }
  }
  return { norm, map };
}

export interface Range {
  start: number;
  end: number;
}

// With several equal matches the one closest to the anchor wins.
export function locateQuote(index: TextIndex, quote: string | null, near: number | null): Range | null {
  if (!quote) return null;
  const q = indexText(quote).norm.trim();
  if (!q) return null;
  let best = -1;
  let from = 0;
  for (;;) {
    const i = index.norm.indexOf(q, from);
    if (i < 0) break;
    const at = index.map[i] ?? 0;
    if (best < 0 || (near !== null && Math.abs(at - near) < Math.abs((index.map[best] ?? 0) - near))) best = i;
    if (near === null) break;
    from = i + 1;
  }
  if (best < 0) return null;
  const last = index.map[best + q.length - 1];
  const first = index.map[best];
  if (first === undefined || last === undefined) return null;
  return { start: first, end: last + 1 };
}
