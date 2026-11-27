import type { Finding } from "../types";

export type Level = "early" | "caveats" | "ready";

export const LEVEL_TITLE: Record<Level, string> = {
  early: "Начинать разработку рано",
  caveats: "Можно начинать с оговорками",
  ready: "Можно начинать разработку",
};
export const LEVEL_SHORT: Record<Level, string> = {
  early: "Начинать рано",
  caveats: "С оговорками",
  ready: "Можно начинать",
};

export interface Counts {
  critical: number;
  major: number;
  minor: number;
}

export function countBySeverity(findings: Finding[]): Counts {
  const c = { critical: 0, major: 0, minor: 0 };
  for (const f of findings) c[f.severity]++;
  return c;
}

export function levelOf(c: Counts): Level {
  return c.critical > 0 ? "early" : c.major > 0 ? "caveats" : "ready";
}

export interface Forecast {
  fixes: number;
  level: Level;
}

// Same rule as the Markdown report: fix all critical, then all major.
export function forecast(c: Counts): Forecast[] {
  const out: Forecast[] = [];
  if (c.critical > 0) out.push({ fixes: c.critical, level: c.major > 0 ? "caveats" : "ready" });
  if (c.major > 0) out.push({ fixes: c.critical + c.major, level: "ready" });
  return out;
}

// Position of a marker on the three-zone scale, in percent.
export const LEVEL_POS: Record<Level, number> = { early: 12, caveats: 50, ready: 86 };

export function plural(n: number, one: string, few: string, many: string): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return one;
  if (m10 >= 2 && m10 <= 4 && (m100 < 10 || m100 >= 20)) return few;
  return many;
}

export const SEV_LABEL = { critical: "Критично", major: "Существенно", minor: "Мелко" } as const;
