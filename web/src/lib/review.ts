import type { Review } from "../types";

// critic_note and file_name are absent in reviews from older servers.
export function normalizeReview(r: Review): Review {
  const spec = { ...r.spec, file_name: r.spec.file_name ?? null };
  if (!r.report) return { ...r, spec };
  return { ...r, spec, report: { ...r.report, findings: r.report.findings.map((f) => ({ ...f, critic_note: f.critic_note ?? null })) } };
}
