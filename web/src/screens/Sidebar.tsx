import { useEffect, useState } from "react";
import { listReviews } from "../api";
import { levelOf, LEVEL_SHORT } from "../lib/readiness";
import { navigate, reviewPath } from "../route";
import type { ReviewSummary } from "../types";

function meta(r: ReviewSummary): { text: string; tone: string } {
  if (r.status === "running") return { text: "Проверяется", tone: "live" };
  if (r.status === "awaiting_answers") return { text: "Ждёт ответов", tone: "live" };
  if (r.status === "failed") return { text: "Ошибка проверки", tone: "red" };
  const level = levelOf({ critical: r.critical ?? 0, major: r.major ?? 0, minor: 0 });
  const tone = level === "early" ? "red" : level === "caveats" ? "amber" : "green";
  return { text: `${LEVEL_SHORT[level]}${r.findings !== null ? ` · ${r.findings} замечаний` : ""}`, tone };
}

// The sidebar: every checked document stays one click away.
export function Sidebar({ activeId, epoch }: { activeId: string | null; epoch: number }) {
  const [items, setItems] = useState<ReviewSummary[]>([]);
  const [q, setQ] = useState("");

  useEffect(() => {
    let alive = true;
    let timer = 0;
    const load = async () => {
      let again = false;
      try {
        const list = await listReviews();
        if (!alive) return;
        setItems(list);
        again = list.some((r) => r.status === "running");
      } catch {
        // the list is secondary: the main screen reports server problems itself
      }
      if (alive && again) timer = window.setTimeout(load, 5000);
    };
    void load();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [activeId, epoch]);

  const needle = q.trim().toLowerCase();
  const shown = needle ? items.filter((r) => [r.title, r.file_name].some((t) => t?.toLowerCase().includes(needle))) : items;

  return (
    <aside className="side">
      <div className="side-top">
        <a className="wordmark" href="#/" aria-label="AnyQ, новая проверка">
          Any<b>Q</b>
        </a>
        <button type="button" className="btn btn-primary btn-sm" onClick={() => navigate("#/")}>
          + Новая проверка
        </button>
      </div>
      <div className="side-title">Проверки</div>
      {items.length > 0 && (
        <input
          className="side-search"
          type="search"
          placeholder="Название или файл"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label="Поиск по проверкам"
        />
      )}
      {shown.length === 0 ? (
        <p className="side-empty">{items.length === 0 ? "Проверок пока нет." : "Ничего не найдено."}</p>
      ) : (
        <nav className="side-list">
          {shown.map((r) => {
            const m = meta(r);
            return (
              <button
                type="button"
                key={r.id}
                className={`side-item ${r.id === activeId ? "on" : ""}`}
                aria-current={r.id === activeId ? "page" : undefined}
                onClick={() => navigate(reviewPath(r.id))}
              >
                <span className="side-name">{r.title ?? r.file_name ?? "Без названия"}</span>
                {r.file_name && <span className="side-file">{r.file_name}</span>}
                <span className={`side-meta tone-${m.tone}`}>{m.text}</span>
              </button>
            );
          })}
        </nav>
      )}
    </aside>
  );
}
