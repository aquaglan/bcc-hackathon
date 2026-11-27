import { useMemo, useState } from "react";
import { buildDoc, indexText, lineForAnchor, locateQuote } from "../lib/doc";
import { countBySeverity, levelOf, LEVEL_SHORT } from "../lib/readiness";
import { isClosed, loadDecisions, OPEN, saveDecisions, type Decision } from "../lib/decisions";
import { navigate, reviewPath, type Tab } from "../route";
import type { Report, Review } from "../types";
import { DocView, hitsByLine } from "./DocView";
import { FindingsTab } from "./FindingsTab";
import { findingKey, type FindingItem, type QuestionItem } from "./model";
import { QuestionsTab } from "./QuestionsTab";
import { Tabs } from "./Tabs";
import { SummaryTab } from "./SummaryTab";

interface Props {
  review: Review;
  report: Report;
  tab: Tab;
  onRechecked: (r: Review) => void;
}

export function Result({ review, report, tab, onRechecked }: Props) {
  const [decisions, setDecisions] = useState(() => loadDecisions(review.id));
  const [selected, setSelected] = useState<number | null>(null);

  const lines = useMemo(() => buildDoc(review.spec.text), [review.spec.text]);
  const index = useMemo(() => indexText(review.spec.text), [review.spec.text]);

  const items: FindingItem[] = useMemo(
    () =>
      report.findings.map((f, i) => {
        const line = lineForAnchor(lines, f.section);
        const key = findingKey(f);
        const range = locateQuote(index, f.quote, line?.start ?? null);
        const decision = decisions[key] ?? OPEN;
        return { i, key, f, line, range, decision, closed: isClosed(decision), at: range?.start ?? line?.start ?? null };
      }),
    [report.findings, lines, index, decisions],
  );
  const questions: QuestionItem[] = useMemo(
    () => report.open_questions.map((q, k) => ({ n: k + 1, q, line: lineForAnchor(lines, q.section) })),
    [report.open_questions, lines],
  );

  const decide = (key: string, d: Decision) => {
    const next = { ...decisions, [key]: d };
    setDecisions(next);
    saveDecisions(review.id, next);
  };
  const open = (i: number) => {
    setSelected(i);
    navigate(reviewPath(review.id, "findings"));
  };

  const left = items.filter((x) => !x.closed);
  const level = levelOf(countBySeverity(left.map((x) => x.f)));
  const counts: Partial<Record<Tab, number>> = { findings: items.length, questions: questions.length };

  return (
    <div className="result">
      <header className="doc-head">
        <span className="doc-title">
          {review.spec.title ?? "Документ"}
          {review.spec.file_name && <span className="doc-file mono">{review.spec.file_name}</span>}
        </span>
        <Tabs
          tab={tab}
          counts={counts}
          onPick={(t) => navigate(reviewPath(review.id, t))}
        />
        {tab === "findings" && (
          <div className="ready-mini">
            <span className={`mini-scale at-${level}`} aria-hidden>
              <span />
              <span />
              <span />
            </span>
            <strong>
              {LEVEL_SHORT[level]}
              {left.length < items.length && ` · закрыто ${items.length - left.length} из ${items.length}`}
            </strong>
          </div>
        )}
      </header>

      <div className="result-body view-in" key={tab}>
        {tab === "summary" && (
          <div className="scroll" data-scroller>
            <SummaryTab report={report} items={items} onOpen={open} />
          </div>
        )}
        {tab === "findings" && (
          <FindingsTab reviewId={review.id} lines={lines} items={items} selected={selected} onSelect={setSelected} onDecide={decide} />
        )}
        {tab === "document" && (
          <div className="scroll" data-scroller>
            <div className="doc-wrap">
              <DocView
                lines={lines}
                hits={hitsByLine(lines, items)}
                selected={selected}
                onSelect={open}
                margin={(line) =>
                  questions
                    .filter((x) => x.line?.global === line.global)
                    .map((x) => (
                      <button
                        type="button"
                        key={x.q.id}
                        className="q-badge"
                        title={x.q.text}
                        onClick={() => navigate(reviewPath(review.id, "questions"))}
                      >
                        В{x.n}
                      </button>
                    ))
                }
              />
            </div>
          </div>
        )}
        {tab === "questions" && (
          <QuestionsTab reviewId={review.id} questions={questions} onRechecked={onRechecked} />
        )}
        {tab === "passport" && (
          <div className="scroll" data-scroller>
            <section className="passport">
              <h2 className="card-title">Паспорт проверки</h2>
              <dl className="kv wide">
                <div>
                  <dt>Модели</dt>
                  <dd>{report.models.join(", ") || "не указаны"}</dd>
                </div>
                <div>
                  <dt>Требований найдено</dt>
                  <dd>{report.requirements.length}</dd>
                </div>
                <div>
                  <dt>Харнесс</dt>
                  <dd>{review.harness}</dd>
                </div>
                <div>
                  <dt>Файл</dt>
                  <dd>{review.spec.file_name ?? "не сохранён"}</dd>
                </div>
                <div>
                  <dt>Номер проверки</dt>
                  <dd className="mono">{review.id}</dd>
                </div>
              </dl>
            </section>
          </div>
        )}
      </div>
    </div>
  );
}
