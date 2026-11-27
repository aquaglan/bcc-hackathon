import { describe, expect, it } from "vitest";
import { parseAnchor } from "./anchors";
import { matches, NO_FILTER, OPEN, sortRows, type Row } from "./decisions";
import { buildDoc, indexText, lineAtOffset, lineForAnchor, locateQuote } from "./doc";
import { fixtureReview } from "./fixture";
import { mdLine, parseInline, parseMarkdown, readerHead } from "./markdown";
import { countBySeverity, forecast, levelOf } from "./readiness";
import { groupBySection } from "./sections";
import { parseRefuted } from "./summary";

const report = fixtureReview.report;
if (!report) throw new Error("fixture has no report");
const text = fixtureReview.spec.text;

describe("anchors", () => {
  it("parses markdown anchors", () => {
    expect(parseAnchor("Часть A › Цели, строка 23")).toEqual({ heading: "Часть A › Цели", page: null, line: 23 });
  });
  it("parses pdf anchors with and without heading", () => {
    expect(parseAnchor("Раздел 2 , стр. 3, строка 4")).toEqual({ heading: "Раздел 2", page: 3, line: 4 });
    expect(parseAnchor("стр. 3, строка 4")).toEqual({ heading: null, page: 3, line: 4 });
  });
  it("handles set-level anchors", () => {
    expect(parseAnchor(null).heading).toBeNull();
    expect(parseAnchor("Весь документ")).toEqual({ heading: "Весь документ", page: null, line: null });
  });
});

describe("quote location", () => {
  const index = indexText(text);
  const lines = buildDoc(text);

  it("finds every quote of the fixture", () => {
    for (const f of report.findings) {
      if (!f.quote) continue;
      const near = lineForAnchor(lines, f.section)?.start ?? null;
      expect(locateQuote(index, f.quote, near), f.quote).not.toBeNull();
    }
  });
  it("prefers the match near the anchor", () => {
    const t = "a\nб повтор\nв\nг повтор\n";
    const l = buildDoc(t);
    const r = locateQuote(indexText(t), "повтор", l[3]?.start ?? 0);
    expect(r && lineAtOffset(l, r.start)?.no).toBe(4);
  });
  it("ignores whitespace and quote style", () => {
    const r = locateQuote(indexText("Он сказал «да»\nи  ушёл"), 'сказал "да" и ушел', null);
    expect(r).toEqual({ start: 3, end: 22 });
  });
  it("returns null for an absent quote", () => {
    expect(locateQuote(index, "такого текста нет", null)).toBeNull();
    expect(locateQuote(index, null, null)).toBeNull();
  });
  it("maps pdf anchors by page", () => {
    const l = buildDoc("p1 l1\np1 l2\fp2 l1\np2 l2");
    expect(lineForAnchor(l, "стр. 2, строка 2")?.text).toBe("p2 l2");
  });
});

describe("readiness", () => {
  const c = countBySeverity(report.findings);
  it("follows the rule on the fixture", () => {
    expect(c).toEqual({ critical: 3, major: 24, minor: 15 });
    expect(levelOf(c)).toBe("early");
    expect(forecast(c)).toEqual([
      { fixes: 3, level: "caveats" },
      { fixes: 27, level: "ready" },
    ]);
  });
  it("skips empty steps", () => {
    expect(forecast({ critical: 2, major: 0, minor: 1 })).toEqual([{ fixes: 2, level: "ready" }]);
    expect(forecast({ critical: 0, major: 4, minor: 0 })).toEqual([{ fixes: 4, level: "ready" }]);
    expect(forecast({ critical: 0, major: 0, minor: 5 })).toEqual([]);
  });
});

describe("sections and summary", () => {
  it("puts every finding into exactly one group", () => {
    const groups = groupBySection(report.findings);
    expect(groups.reduce((n, g) => n + g.findings.length, 0)).toBe(report.findings.length);
    expect(new Set(groups.map((g) => g.heading)).size).toBe(groups.length);
  });
  it("parses the refuted count", () => {
    expect(parseRefuted(report.summary)).toBe(1);
    expect(parseRefuted("без числа")).toBeNull();
  });
});

describe("markdown", () => {
  it("parses headings, tables, list items and paragraphs", () => {
    const md = "# Title\n\n| Поле | Значение |\n|---|:--|\n| Продукт | **Банк** |\n\n- пункт\n1. второй\nтекст\n---";
    expect(parseMarkdown(md)).toEqual([
      { kind: "h", level: 1, text: "Title" },
      { kind: "table", head: ["Поле", "Значение"], rows: [["Продукт", "**Банк**"]] },
      { kind: "li", text: "пункт" },
      { kind: "li", text: "второй" },
      { kind: "p", text: "текст" },
    ]);
  });
  it("keeps a table without a header rule as plain rows", () => {
    expect(parseMarkdown("| a | b |\n| c | d |")).toEqual([{ kind: "table", head: null, rows: [["a", "b"], ["c", "d"]] }]);
  });
  it("splits bold and code", () => {
    expect(parseInline("a **b** `c`")).toEqual([{ text: "a " }, { text: "b", strong: true }, { text: " " }, { text: "c", code: true }]);
  });
  it("sweeps while extracting and stays in the current stretch while checking", () => {
    expect([0, 1, 10, 11].map((t) => readerHead(10, t, true, 0, 0))).toEqual([0, 1, 0, 1]);
    for (let t = 0; t < 20; t++) {
      const h = readerHead(101, t, false, 10, 3);
      expect(h).toBeGreaterThanOrEqual(30);
      expect(h).toBeLessThanOrEqual(40);
    }
    expect(readerHead(101, 7, false, 10, 10)).toBe(100);
  });
});

describe("markdown lines", () => {
  it("hides heading and bold markup but keeps offsets", () => {
    const h = mdLine("## Цели **важно**");
    expect([h.kind, h.level, h.body]).toEqual(["h", 2, [3, 17]]);
    expect(h.hidden).toEqual([[8, 10], [15, 17]]);
    expect(h.strong).toEqual([[10, 15]]);
  });
  it("splits a table row into trimmed cells and knows the separator", () => {
    const t = "| **Продукт** | Банк |";
    const r = mdLine(t);
    expect(r.kind).toBe("row");
    expect(r.cells.map(([a, b]) => t.slice(a, b))).toEqual(["**Продукт**", "Банк"]);
    expect(mdLine("| :--- | :--- |").kind).toBe("sep");
    expect(mdLine("---").kind).toBe("rule");
    expect(mdLine("- пункт").body).toEqual([2, 7]);
  });
});

describe("decisions", () => {
  const f = (severity: "critical" | "major" | "minor", category: string, problem: string, verified = false) => ({
    requirement_id: null,
    severity,
    category,
    section: null,
    quote: null,
    problem,
    suggestion: "",
    verified,
    critic_note: null,
  });
  const rows: Row[] = [
    { i: 0, f: f("minor", "B", "Нет единицы"), decision: OPEN, at: 10 },
    { i: 1, f: f("critical", "A", "Противоречие", true), decision: { status: "rejected", fix: null, comment: "не баг" }, at: 50 },
    { i: 2, f: f("major", "A", "Порог не задан"), decision: { status: "edited", fix: "Порог 50 000 ₸", comment: null }, at: null },
  ];
  const ids = (r: Row[]) => r.map((x) => x.i);

  it("filters by text, including own fixes and comments, ignoring case and ё", () => {
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, q: "порог" })))).toEqual([2]);
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, q: "НЕ БАГ" })))).toEqual([1]);
  });
  it("filters by severity, category, status and critic", () => {
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, severities: ["critical", "minor"] })))).toEqual([0, 1]);
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, category: "A" })))).toEqual([1, 2]);
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, status: "open" })))).toEqual([0]);
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, status: "closed" })))).toEqual([1, 2]);
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, status: "edited" })))).toEqual([2]);
    expect(ids(rows.filter((r) => matches(r, { ...NO_FILTER, verified: "yes" })))).toEqual([1]);
  });
  it("sorts by severity, document order, category and status", () => {
    expect(ids(sortRows(rows, "severity"))).toEqual([1, 2, 0]);
    expect(ids(sortRows(rows, "document"))).toEqual([0, 1, 2]);
    expect(ids(sortRows(rows, "category"))).toEqual([1, 2, 0]);
    expect(ids(sortRows(rows, "status"))).toEqual([0, 2, 1]);
  });
});

describe("decision storage", () => {
  it("carries over findings accepted before decisions existed", async () => {
    const mem = new Map<string, string>();
    Object.assign(globalThis, {
      localStorage: { getItem: (k: string) => mem.get(k) ?? null, setItem: (k: string, v: string) => mem.set(k, v), removeItem: (k: string) => mem.delete(k) },
    });
    const { loadDecisions, saveDecisions } = await import("./decisions");
    mem.set("anyq:accepted:r1", JSON.stringify(["k1"]));
    expect(loadDecisions("r1")).toEqual({ k1: { status: "accepted", fix: null, comment: null } });
    saveDecisions("r1", { k2: { status: "rejected", fix: null, comment: "нет" } });
    expect(loadDecisions("r1")).toEqual({ k2: { status: "rejected", fix: null, comment: "нет" } });
  });
});
