export type MdBlock =
  | { kind: "h"; level: number; text: string }
  | { kind: "table"; head: string[] | null; rows: string[][] }
  | { kind: "li"; text: string }
  | { kind: "p"; text: string };

const isRow = (l: string) => l.startsWith("|");
const isRule = (l: string) => /^\|?[\s:|-]*-[\s:|-]*$/.test(l);
const cells = (l: string) =>
  l
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());

// Block-level markdown only: what the reader shows while the document is being checked.
export function parseMarkdown(text: string): MdBlock[] {
  const lines = text.split(/\n|\f/).map((l) => l.trim());
  const out: MdBlock[] = [];
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i] as string;
    if (l === "" || /^([-*_]\s*){3,}$/.test(l)) continue;
    if (isRow(l)) {
      const block: string[] = [];
      while (i < lines.length && isRow(lines[i] as string)) block.push(lines[i++] as string);
      i--;
      const ruled = block.length > 1 && isRule(block[1] as string);
      const rows = block.filter((r) => !isRule(r)).map(cells);
      out.push(ruled ? { kind: "table", head: rows[0] ?? null, rows: rows.slice(1) } : { kind: "table", head: null, rows });
      continue;
    }
    const h = /^(#{1,6})\s+(.*)$/.exec(l);
    if (h) {
      out.push({ kind: "h", level: (h[1] as string).length, text: (h[2] as string).replace(/\s#+$/, "") });
      continue;
    }
    const li = /^(?:[-*+]|\d+[.)])\s+(.*)$/.exec(l);
    if (li) {
      out.push({ kind: "li", text: li[1] as string });
      continue;
    }
    out.push({ kind: "p", text: l.replace(/^>\s?/, "") });
  }
  return out;
}

export interface Inline {
  text: string;
  strong?: boolean;
  code?: boolean;
}

export function parseInline(text: string): Inline[] {
  return text
    .split(/(\*\*[^*]+\*\*|`[^`]+`)/)
    .filter((t) => t !== "")
    .map((t) =>
      t.startsWith("**") && t.endsWith("**") && t.length > 4
        ? { text: t.slice(2, -2), strong: true }
        : t.startsWith("`") && t.endsWith("`") && t.length > 2
          ? { text: t.slice(1, -1), code: true }
          : { text: t },
    );
}

// Where the reading band stands. Extraction has no count yet, so the band sweeps the whole
// document on the clock; checking keeps it inside the stretch of the requirement being checked.
// shortcut: requirements are assumed to follow document order; use the requirement's own line once progress reports it.
export function readerHead(units: number, tick: number, extracting: boolean, total: number, checked: number): number {
  if (units <= 1) return 0;
  if (extracting || total <= 0) return tick % units;
  const at = (n: number) => Math.floor((Math.min(n, total) / total) * (units - 1));
  const from = at(checked);
  return from + (tick % (at(checked + 1) - from + 1));
}

export type Span = [number, number];

// One source line seen as markdown, with ranges relative to the line, so the document view
// keeps its line numbers and quote offsets while hiding the markup.
export interface MdLine {
  kind: "h" | "li" | "rule" | "sep" | "row" | "text";
  level: number;
  body: Span;
  cells: Span[];
  hidden: Span[];
  strong: Span[];
  code: Span[];
}

export function mdLine(text: string): MdLine {
  const line: MdLine = { kind: "text", level: 0, body: [0, text.length], cells: [], hidden: [], strong: [], code: [] };
  if (/^\s*\|?[\s:|-]*-[\s:|-]*\|?\s*$/.test(text) && text.includes("|")) return { ...line, kind: "sep" };
  if (/^\s*([-*_]\s*){3,}$/.test(text)) return { ...line, kind: "rule" };
  for (const m of text.matchAll(/\*\*(?=\S)(.+?)\*\*/g)) {
    const at = m.index ?? 0;
    const end = at + m[0].length;
    line.hidden.push([at, at + 2], [end - 2, end]);
    line.strong.push([at + 2, end - 2]);
  }
  for (const m of text.matchAll(/`([^`]+)`/g)) {
    const at = m.index ?? 0;
    const end = at + m[0].length;
    line.hidden.push([at, at + 1], [end - 1, end]);
    line.code.push([at + 1, end - 1]);
  }
  const h = /^\s*(#{1,6})\s+/.exec(text);
  if (h) return { ...line, kind: "h", level: (h[1] as string).length, body: [h[0].length, text.length] };
  const li = /^\s*[-*+]\s+/.exec(text);
  if (li) return { ...line, kind: "li", body: [li[0].length, text.length] };
  if (/^\s*\|/.test(text)) {
    const pipes = [...text.matchAll(/(?<!\\)\|/g)].map((m) => m.index ?? 0);
    const tail = text.slice((pipes[pipes.length - 1] ?? 0) + 1).trim() !== "";
    const bounds = tail ? [...pipes, text.length] : pipes;
    for (let k = 0; k + 1 < bounds.length; k++) {
      let a = (bounds[k] as number) + 1;
      let b = bounds[k + 1] as number;
      while (a < b && text[a] === " ") a++;
      while (b > a && text[b - 1] === " ") b--;
      line.cells.push([a, b]);
    }
    return { ...line, kind: "row" };
  }
  return line;
}
