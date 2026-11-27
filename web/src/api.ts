import { normalizeReview } from "./lib/review";
import type { Answer, Progress, Review, ReviewSummary, Spec } from "./types";

export class ApiError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

// Dev-only: `?fixture` serves the recorded review, `?fixture=running` a running one.
const fixtureMode = import.meta.env.DEV ? new URLSearchParams(location.search).get("fixture") : null;
const fixtureStart = Date.now();

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    throw new ApiError("network", "Сервер недоступен. Проверьте, что он запущен.", 0);
  }
  if (!res.ok) {
    let code = "http";
    let message = `Ошибка сервера (${res.status})`;
    try {
      const body = (await res.json()) as { code?: string; message?: string };
      if (body.code) code = body.code;
      if (body.message) message = body.message;
    } catch {
      // non-JSON error body: keep the generic message
    }
    throw new ApiError(code, message, res.status);
  }
  return (await res.json()) as T;
}

const json = (body: unknown): RequestInit => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
});

function devReadOnly(): never {
  throw new ApiError("dev", "В режиме fixture запись недоступна", 0);
}

export async function uploadDocument(file: File): Promise<Spec> {
  if (fixtureMode !== null) devReadOnly();
  const doc = await request<{ title: string; text: string }>(`/v1/documents?name=${encodeURIComponent(file.name)}`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: file,
  });
  return { ...doc, file_name: file.name };
}

export async function createReview(spec: Spec): Promise<Review> {
  if (fixtureMode !== null) devReadOnly();
  return normalizeReview(await request<Review>("/v1/reviews", json({ spec, harness: null })));
}

export async function listReviews(): Promise<ReviewSummary[]> {
  if (fixtureMode !== null) {
    const { fixtureReview: r } = await import("./lib/fixture");
    return [{ id: r.id, title: r.spec.title, file_name: "notification-spend.md", status: "done", findings: r.report?.findings.length ?? null, critical: 3, major: 24 }];
  }
  // file_name is absent from older servers
  return (await request<ReviewSummary[]>("/v1/reviews")).map((r) => ({ ...r, file_name: r.file_name ?? null }));
}

export async function getReview(id: string): Promise<Review> {
  if (fixtureMode !== null) {
    const { fixtureReview: r } = await import("./lib/fixture");
    const named = { ...r, spec: { ...r.spec, file_name: "notification-spend.md" } };
    return fixtureMode === "running" ? { ...named, status: "running", report: null } : named;
  }
  return normalizeReview(await request<Review>(`/v1/reviews/${encodeURIComponent(id)}`));
}

export async function getProgress(id: string): Promise<Progress> {
  if (fixtureMode === "running") {
    // The recorded run advances with time so the live screen can be seen moving.
    const t = Math.floor((Date.now() - fixtureStart) / 1000);
    const found = Math.floor(t / 3);
    return {
      stage: t < 25 ? "check" : "critic",
      requirements: 60,
      checked: Math.min(60, 23 + t * 2),
      critical: 1 + Math.floor(found / 4),
      major: 6 + Math.floor(found / 2),
      minor: 4 + found,
      events: [
        { kind: "stage", severity: null, text: "Этап: проверка требований", at_ms: 1200 },
        { kind: "found", severity: "major", text: "R-14: нет срока возврата", at_ms: 8300 },
        { kind: "dropped", severity: null, text: "Повтор замечания отброшен", at_ms: 9100 },
        ...Array.from({ length: found }, (_, k) => ({
          kind: "found" as const,
          severity: (["minor", "major", "minor", "critical"] as const)[k % 4] ?? "minor",
          text: `R-${20 + k}: ${["нет единицы измерения", "порог не задан числом", "нет поведения при ошибке", "противоречит критерию приёмки"][k % 4]}`,
          at_ms: 10000 + k * 3000,
        })),
      ],
    };
  }
  if (fixtureMode !== null) throw new ApiError("not_found", "нет прогресса", 404);
  return request<Progress>(`/v1/reviews/${encodeURIComponent(id)}/progress`);
}

export async function sendAnswers(id: string, answers: Answer[]): Promise<Review> {
  if (fixtureMode !== null) devReadOnly();
  return normalizeReview(await request<Review>(`/v1/reviews/${encodeURIComponent(id)}/answers`, json({ answers })));
}
