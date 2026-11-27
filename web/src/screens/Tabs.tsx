import { useLayoutEffect, useRef, useState } from "react";
import type { Tab } from "../route";

const TAB_LABEL: Record<Tab, string> = {
  summary: "Итог",
  findings: "Замечания",
  document: "Документ",
  questions: "Вопросы",
  passport: "Паспорт",
};

// Segmented tabs: one ink pill slides under the active tab instead of each tab repainting.
export function Tabs({ tab, counts, onPick }: { tab: Tab; counts: Partial<Record<Tab, number>>; onPick: (t: Tab) => void }) {
  const nav = useRef<HTMLElement>(null);
  const [pill, setPill] = useState<{ x: number; w: number } | null>(null);

  useLayoutEffect(() => {
    const measure = () => {
      const el = nav.current?.querySelector<HTMLElement>(`[data-tab="${tab}"]`);
      if (el) setPill({ x: el.offsetLeft, w: el.offsetWidth });
    };
    measure();
    void document.fonts.ready.then(measure);
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [tab, counts.findings, counts.questions]);

  return (
    <nav className="tabs" ref={nav}>
      {pill && <span className="tabs-pill" style={{ transform: `translateX(${pill.x}px)`, width: pill.w }} aria-hidden />}
      {(Object.keys(TAB_LABEL) as Tab[]).map((t) => (
        <button
          type="button"
          key={t}
          data-tab={t}
          className={`tab ${t === tab ? "on" : ""}`}
          aria-current={t === tab ? "page" : undefined}
          onClick={() => onPick(t)}
        >
          {TAB_LABEL[t]}
          {counts[t] !== undefined && <span className="tab-n">{counts[t]}</span>}
        </button>
      ))}
    </nav>
  );
}
