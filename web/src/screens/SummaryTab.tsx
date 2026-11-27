import { parseAnchor } from "../lib/anchors";
import { countBySeverity, forecast, levelOf, LEVEL_POS, LEVEL_TITLE, plural, type Level } from "../lib/readiness";
import { groupBySection } from "../lib/sections";
import { parseRefuted } from "../lib/summary";
import type { Report } from "../types";
import type { FindingItem } from "./model";

function Scale({ level, steps }: { level: Level; steps: { fixes: number; level: Level }[] }) {
  return (
    <div className="scale">
      <div className="scale-bar">
        <span />
        <span />
        <span />
      </div>
      <div className="scale-names">
        <span>Рано</span>
        <span>С оговорками</span>
        <span>Можно начинать</span>
      </div>
      {steps.map((s) => (
        <div key={s.fixes} className={`marker forecast f-${s.level}`} style={{ left: `${LEVEL_POS[s.level]}%` }}>
          <span className="marker-tag">
            после {s.fixes} {plural(s.fixes, "правки", "правок", "правок")}
          </span>
          <span className="marker-dot" />
        </div>
      ))}
      <div className="marker now" style={{ left: `${LEVEL_POS[level]}%` }}>
        <span className="marker-tag">Сейчас</span>
        <span className={`marker-dot now-${level}`} />
      </div>
    </div>
  );
}

const LIST = 10;

interface Props {
  report: Report;
  items: FindingItem[];
  onOpen: (i: number) => void;
}

export function SummaryTab({ report, items, onOpen }: Props) {
  const left = items.filter((x) => !x.closed);
  const counts = countBySeverity(left.map((x) => x.f));
  const level = levelOf(counts);
  const blockers = left.filter((x) => x.f.severity === (counts.critical > 0 ? "critical" : "major"));
  const shown = blockers.length > 0 ? blockers : left;
  const groups = groupBySection(items.map((x) => x.f));
  const byFinding = new Map(items.map((x) => [x.f, x]));

  const serious = items.filter((x) => x.f.severity !== "minor");
  const verified = serious.filter((x) => x.f.verified).length;
  const refuted = parseRefuted(report.summary);

  return (
    <div className="summary">
      <div className="sum-top">
        <section className={`sum-card v-${level}`}>
          <div className="card-head">
            <span className="card-label">Вердикт</span>
            <span className="sum-counts">
              <b className="sev-ink-critical">{counts.critical}</b> критичных · <b className="sev-ink-major">{counts.major}</b> существенных ·{" "}
              <b>{counts.minor}</b> мелких
            </span>
          </div>
          <h1 className="sum-verdict">{LEVEL_TITLE[level]}</h1>
          <Scale level={level} steps={forecast(counts)} />
          {items.length > left.length && <p className="muted small">Закрыто замечаний: {items.length - left.length}. Шкала пересчитана без них.</p>}
        </section>

        <section className="sum-card">
          <div className="card-head">
            <span className="card-label">Перепроверка критиком</span>
          </div>
          <p className="muted small">Из {serious.length} критичных и существенных замечаний</p>
          <div className="sum-stats">
            <div>
              <b className="good">{verified}</b>
              <span>подтвердил</span>
            </div>
            <div>
              <b>{serious.length - verified}</b>
              <span>не проверял</span>
            </div>
            {refuted !== null && (
              <div>
                <b>{refuted}</b>
                <span>опроверг и убрал</span>
              </div>
            )}
          </div>
        </section>
      </div>

      <div className="sum-grid">
        <section className="sum-card">
          <div className="card-head">
            <span className="card-title">{counts.critical > 0 ? "Что мешает начать" : "Что стоит поправить"}</span>
            <span className="muted small">{shown.length}</span>
            {shown[0] && (
              <button type="button" className="btn btn-primary btn-sm push" onClick={() => onOpen((shown[0] as FindingItem).i)}>
                Разобрать по порядку
              </button>
            )}
          </div>
          {shown.length === 0 && <div className="empty-row">Замечаний не осталось.</div>}
          {shown.slice(0, LIST).map((x, n) => (
            <button type="button" className="blocker" key={x.i} onClick={() => onOpen(x.i)}>
              <span className={`num-chip sev-${x.f.severity}`}>{n + 1}</span>
              <span className="blocker-body">
                <span className="blocker-text">
                  {x.f.quote && (
                    <>
                      «<span className={`quote sev-${x.f.severity}`}>{x.f.quote.replace(/\*\*/g, "")}</span>» —{" "}
                    </>
                  )}
                  {x.f.problem}
                </span>
                {x.f.section && <span className="blocker-place">{x.f.section}</span>}
              </span>
            </button>
          ))}
          {shown.length > LIST && <div className="empty-row">И ещё {shown.length - LIST} во вкладке «Замечания».</div>}
        </section>

        <div className="sum-col">
          <section className="sum-card">
            <div className="card-head">
              <span className="card-title">Заключение агента</span>
            </div>
            <p className="agent-summary">{report.summary}</p>
          </section>

          <section className="sum-card">
            <div className="card-head">
              <span className="card-title">Где проблемы</span>
              <span className="muted small push">по разделам ТЗ</span>
            </div>
            <div className="groups">
              {groups.map((g) => {
                const parts = g.heading.split(" › ");
                const title = parts[parts.length - 1] ?? g.heading;
                const remaining = g.findings.filter((f) => !byFinding.get(f)?.closed);
                const first = byFinding.get(g.findings[0] as (typeof g.findings)[number]);
                const sub = parts.length > 1 ? parts.slice(0, -1).join(" › ") : g.firstPage ? `стр. ${g.firstPage}` : null;
                const page = parseAnchor(g.findings[0]?.section ?? null).page;
                return (
                  <button type="button" className="group" key={g.heading} title={g.heading} onClick={() => first && onOpen(first.i)}>
                    <span className="group-main">
                      <span className={`group-title ${remaining.some((f) => f.severity === "critical") ? "strong" : ""}`}>{title}</span>
                      {(sub || page) && <span className="group-sub">{sub ?? `стр. ${page}`}</span>}
                    </span>
                    <span className="dots">
                      {remaining.map((f, k) => (
                        <span key={k} className="dot" style={{ background: `var(--sev-${f.severity})` }} />
                      ))}
                      {remaining.length === 0 && <span className="all-ok">приняты</span>}
                    </span>
                  </button>
                );
              })}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
