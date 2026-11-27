export interface Anchor {
  heading: string | null;
  page: number | null;
  line: number | null;
}

// Anchors are produced by the server as Russian strings; see the API contract.
export function parseAnchor(section: string | null): Anchor {
  if (!section) return { heading: null, page: null, line: null };
  const pl = /стр\.\s*(\d+),\s*строка\s*(\d+)/.exec(section);
  const l = /строка\s*(\d+)/.exec(section);
  const pageOnly = /стр\.\s*(\d+)/.exec(section);
  const cut = /\s*,?\s*(?:стр\.\s*\d+|строка\s*\d+)/.exec(section);
  const heading = (cut ? section.slice(0, cut.index) : section).trim();
  return {
    heading: heading || null,
    page: pl ? Number(pl[1]) : pageOnly ? Number(pageOnly[1]) : null,
    line: pl ? Number(pl[2]) : l ? Number(l[1]) : null,
  };
}
