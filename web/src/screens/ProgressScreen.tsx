import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { parseInline, parseMarkdown, readerHead } from "../lib/markdown";
import { getProgress } from "../api";
import { load, save } from "../lib/store";
import { Count, gsap, MOTION_OK, Scribble, SplitHeading, useGSAP } from "../motion";
import type { Progress, ProgressEvent, Review, Severity, Stage } from "../types";

const STAGES: { id: Stage; label: string }[] = [
  { id: "extract", label: "Разбор на требования" },
  { id: "check", label: "Проверка требований и набора" },
  { id: "critic", label: "Перепроверка критиком" },
  { id: "questions", label: "Сборка вопросов" },
  { id: "done", label: "Готово" },
];

function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function ProgressScreen({ review }: { review: Review }) {
  const [progress, setProgress] = useState<Progress | null>(null);
  const [now, setNow] = useState(Date.now());
  const [started] = useState(() => {
    const key = `anyq:started:${review.id}`;
    const t = load<number | null>(key, null);
    if (t !== null) return t;
    save(key, Date.now());
    return Date.now();
  });

  useEffect(() => {
    let alive = true;
    let timer = 0;
    const tick = async () => {
      try {
        const p = await getProgress(review.id);
        if (alive) setProgress(p);
      } catch {
        // progress is optional: without it only the elapsed time is shown
        if (alive) setProgress(null);
      }
      if (alive) timer = window.setTimeout(tick, 1000);
    };
    void tick();
    const clockTimer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => {
      alive = false;
      window.clearTimeout(timer);
      window.clearInterval(clockTimer);
    };
  }, [review.id]);

  const stageIdx = progress ? STAGES.findIndex((s) => s.id === progress.stage) : -1;
  const total = progress?.requirements ?? 0;
  const percent = progress && total > 0 ? Math.min(100, (progress.checked / total) * 100) : 0;
  const feed = progress ? [...progress.events].reverse().slice(0, 7) : [];
  const lastHit = feed.find((e) => e.kind === "found");
  const heading = progress?.stage === "extract" ? "Разбираю документ на требования" : "Идёт проверка";

  return (
    <div className="progress night" data-scroller>
      <div className="progress-top">
        <span className="progress-doc">{review.spec.title ?? "Документ"}</span>
        <span className="elapsed">
          прошло <b>{clock(now - started)}</b>
        </span>
      </div>

      <div className="progress-grid">
        <div className="progress-main">
          {progress && total > 0 ? (
            <h1 className="big-count" aria-label={`Проверено ${progress.checked} из ${total} требований`}>
              <Count value={progress.checked} />
              <span className="thin">/{total}</span>
            </h1>
          ) : (
            // keyed: SplitHeading splits its text once per mount
            <SplitHeading key={heading} className="progress-h">
              {heading}
            </SplitHeading>
          )}
          {progress && total > 0 && <p className="big-count-label">требований проверено</p>}
          <div className="line" role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={progress?.checked ?? 0}>
            <span style={{ transform: `scaleX(${percent / 100})` }} />
          </div>
          {!progress && <p className="progress-note">Подробности этапов сейчас недоступны. Проверка идёт, страница обновится сама.</p>}

          {progress && (
            <div className="tally">
              <div className="tally-cell t-critical">
                <Count value={progress.critical} className="tally-n" />
                <span>критичных</span>
              </div>
              <div className="tally-cell t-major">
                <Count value={progress.major} className="tally-n" />
                <span>существенных</span>
              </div>
              <div className="tally-cell t-minor">
                <Count value={progress.minor} className="tally-n" />
                <span>мелких</span>
              </div>
            </div>
          )}
          {progress && <p className="tally-note">До перепроверки критиком: итоговое число может быть меньше.</p>}
        </div>

        <Reader text={review.spec.text} extracting={!progress || progress.stage === "extract"} total={total} checked={progress?.checked ?? 0} hit={lastHit ?? null} />

        <div className="progress-side">
          <ol className="stages">
            {STAGES.map((s, i) => {
              const state = stageIdx < 0 ? "todo" : i < stageIdx || (s.id === "done" && stageIdx === i) ? "done" : i === stageIdx ? "cur" : "todo";
              return (
                <li key={s.id} className={`stage ${state}`} aria-current={state === "cur" ? "step" : undefined}>
                  <span className="stage-no">{String(i + 1).padStart(2, "0")}</span>
                  <span className="stage-name">{s.label}</span>
                  <span className="stage-mark" aria-hidden>
                    {state === "done" && (
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M5 12.5l4.5 4.5L19 7.5" pathLength={1} />
                      </svg>
                    )}
                  </span>
                </li>
              );
            })}
          </ol>
          <div className="feed" aria-live="polite">
            {feed.map((e) => (
              <FeedRow key={`${e.at_ms}-${e.text}`} kind={e.kind} severity={e.severity} text={e.text} at={clock(e.at_ms)} />
            ))}
          </div>
          <Scribble className="progress-scribble" delay={1.4} arrow={false}>
            ( можно уйти — страница обновится сама )
          </Scribble>
        </div>
      </div>
    </div>
  );
}

// The document itself, rendered as markdown; the band walks it block by block.
function Reader({ text, extracting, total, checked, hit }: { text: string; extracting: boolean; total: number; checked: number; hit: ProgressEvent | null }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  const units = blocks.reduce((n, b) => n + (b.kind === "table" ? b.rows.length + (b.head ? 1 : 0) : 1), 0);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!window.matchMedia(MOTION_OK).matches) return;
    const t = window.setInterval(() => setTick((n) => n + 1), 900);
    return () => window.clearInterval(t);
  }, []);
  const head = readerHead(units, tick, extracting, total, checked);

  const docRef = useRef<HTMLDivElement>(null);
  const [shift, setShift] = useState(0);
  useLayoutEffect(() => {
    const doc = docRef.current;
    const el = doc?.querySelector(`[data-u="${head}"]`);
    if (doc && el) setShift(el.getBoundingClientRect().top - doc.getBoundingClientRect().top);
  }, [head, blocks]);

  let u = 0;
  const mark = () => {
    const k = u++;
    return { "data-u": k, className: k < head ? "read" : k === head ? "here" : undefined };
  };
  return (
    <div className="reader" aria-hidden>
      <div className="reader-doc" ref={docRef} style={{ transform: `translateY(${-shift}px)` }}>
        {blocks.map((b, i) => {
          if (b.kind === "h") {
            const m = mark();
            return (
              <div key={i} {...m} className={`rd-h rd-h${Math.min(b.level, 3)} ${m.className ?? ""}`}>
                <Md text={b.text} />
              </div>
            );
          }
          if (b.kind === "table")
            return (
              <table key={i} className="rd-table">
                {b.head && (
                  <thead>
                    <tr {...mark()}>
                      {b.head.map((c, j) => (
                        <th key={j}>
                          <Md text={c} />
                        </th>
                      ))}
                    </tr>
                  </thead>
                )}
                <tbody>
                  {b.rows.map((r, k) => (
                    <tr key={k} {...mark()}>
                      {r.map((c, j) => (
                        <td key={j}>
                          <Md text={c} />
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            );
          const m = mark();
          return (
            <p key={i} {...m} className={`rd-${b.kind} ${m.className ?? ""}`}>
              <Md text={b.text} />
            </p>
          );
        })}
      </div>
      {hit && (
        <div className={`reader-hit h-${hit.severity ?? "minor"}`} key={`${hit.at_ms}-${hit.text}`}>
          {hit.text}
        </div>
      )}
    </div>
  );
}

function Md({ text }: { text: string }) {
  return (
    <>
      {parseInline(text).map((s, k) => (s.strong ? <strong key={k}>{s.text}</strong> : s.code ? <code key={k}>{s.text}</code> : <span key={k}>{s.text}</span>))}
    </>
  );
}

// A new event rises from under its row's mask; old rows keep their place.
function FeedRow({ kind, severity, text, at }: { kind: ProgressEvent["kind"]; severity: Severity | null; text: string; at: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add(MOTION_OK, () => {
        gsap.from(".feed-in", { yPercent: 110, duration: 0.9, ease: "expo.out" });
      });
      return () => mm.revert();
    },
    { scope: ref },
  );
  return (
    <div className="feed-row" ref={ref}>
      <div className="feed-in">
        <span className={`dot sev-dot-${kind === "found" && severity ? severity : kind}`} />
        <span className={`feed-text ${kind === "stage" ? "dim" : ""}`}>{text}</span>
        <span className="mono feed-time">{at}</span>
      </div>
    </div>
  );
}
