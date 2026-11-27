import { useState } from "react";
import { sendAnswers } from "../api";
import { parseAnchor } from "../lib/anchors";
import { mdLine } from "../lib/markdown";
import { drop, load, save } from "../lib/store";
import { notifyError } from "../toast";
import type { Review } from "../types";
import type { QuestionItem } from "./model";

// The server marks the first 12 questions as the main ones; the API does not expose that number.
// shortcut: mirrors limits.main_questions of the agent, change both together; expose it in the report when it becomes configurable per run.
const MAIN_QUESTIONS = 12;

interface Draft {
  text: string;
  skipped: boolean;
}
type Drafts = Record<string, Draft>;

interface Props {
  reviewId: string;
  questions: QuestionItem[];
  onRechecked: (r: Review) => void;
}

type Show = "all" | "todo" | "answered" | "skipped";

function stateOf(d: Draft | undefined): Exclude<Show, "all"> {
  if ((d?.text ?? "").trim()) return "answered";
  return d?.skipped ? "skipped" : "todo";
}

// A source line without its markup: table cells joined, markers dropped.
function plain(text: string): string {
  const md = mdLine(text);
  const body = md.kind === "row" ? md.cells.map(([a, b]) => text.slice(a, b)).join(" · ") : text.slice(md.body[0]);
  return body.replace(/\*\*|`/g, "");
}

function Row({ item, draft, onChange }: { item: QuestionItem; draft: Draft | undefined; onChange: (d: Draft) => void }) {
  const text = draft?.text ?? "";
  const place = parseAnchor(item.q.section).heading;
  const state = stateOf(draft);
  return (
    <section className={`sum-card qrow q-${state}`} id={`q-${item.q.id}`}>
      <div className="qrow-top">
        <span className="qrow-no mono">В{String(item.n).padStart(2, "0")}</span>
        {place && <span className="qrow-place mono">{place}</span>}
        <span className={`st st-q-${state} push`}>{state === "answered" ? "Есть ответ" : state === "skipped" ? "Пропущен" : "Без ответа"}</span>
      </div>
      <p className="qrow-text">{item.q.text}</p>
      <p className="q-why">
        <b>Зачем:</b> {item.q.reason}
      </p>
      {item.line && item.line.text.trim() !== "" && (
        <blockquote className="q-src">
          <span className="mono">строка {item.line.global}</span> {plain(item.line.text)}
        </blockquote>
      )}
      {draft?.skipped ? (
        <div className="q-skip">
          Пропущено: агент оставит это открытым вопросом в отчёте.
          <button type="button" className="link-btn" onClick={() => onChange({ text: "", skipped: false })}>
            Ответить
          </button>
        </div>
      ) : (
        <div className="q-answer">
          <textarea
            className="input"
            rows={2}
            placeholder="Ответ"
            aria-label={`Ответ на вопрос В${item.n}`}
            value={text}
            onChange={(e) => onChange({ text: e.target.value, skipped: false })}
          />
          {!text.trim() && (
            <button type="button" className="link-btn" onClick={() => onChange({ text: "", skipped: true })}>
              Пропустить
            </button>
          )}
        </div>
      )}
    </section>
  );
}

export function QuestionsTab({ reviewId, questions, onRechecked }: Props) {
  const storeKey = `anyq:answers:${reviewId}`;
  const [drafts, setDrafts] = useState<Drafts>(() => load<Drafts>(storeKey, {}));
  const [showRest, setShowRest] = useState(false);
  const [busy, setBusy] = useState(false);
  const [q, setQ] = useState("");
  const [show, setShow] = useState<Show>("all");

  const change = (id: string, d: Draft) => {
    const next = { ...drafts, [id]: d };
    setDrafts(next);
    save(storeKey, next);
  };

  const answers = questions.filter((x) => stateOf(drafts[x.q.id]) === "answered");
  const skipped = questions.filter((x) => stateOf(drafts[x.q.id]) === "skipped").length;
  const send = async () => {
    setBusy(true);
    try {
      const review = await sendAnswers(
        reviewId,
        answers.map((x) => ({ question_id: x.q.id, text: (drafts[x.q.id]?.text ?? "").trim() })),
      );
      drop(storeKey);
      save(`anyq:started:${review.id}`, Date.now());
      onRechecked(review);
    } catch (e) {
      notifyError(e);
      setBusy(false);
    }
  };

  const rest = questions.length - MAIN_QUESTIONS;
  const needle = q.trim().toLowerCase();
  const visible = questions.filter(
    (x) =>
      (x.n <= MAIN_QUESTIONS || showRest) &&
      (show === "all" || stateOf(drafts[x.q.id]) === show) &&
      (!needle || [x.q.text, x.q.reason, x.q.section, drafts[x.q.id]?.text].some((t) => t?.toLowerCase().includes(needle))),
  );

  if (questions.length === 0) return <div className="center-screen">Агент не задал вопросов.</div>;

  return (
    <div className="qtab">
      <div className="qbar">
        <input className="input qbar-search" type="search" placeholder="Поиск по вопросам" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Поиск по вопросам" />
        <select className="input qbar-select" value={show} onChange={(e) => setShow(e.target.value as Show)} aria-label="Какие вопросы показать">
          <option value="all">Все</option>
          <option value="todo">Без ответа</option>
          <option value="answered">С ответом</option>
          <option value="skipped">Пропущенные</option>
        </select>
        {rest > 0 && (
          <label className="qbar-check">
            <input type="checkbox" checked={showRest} onChange={(e) => setShowRest(e.target.checked)} />
            Второстепенные ({rest})
          </label>
        )}
        <span className="muted small push">
          Ответов {answers.length} из {questions.length}
          {skipped > 0 && ` · пропущено ${skipped}`}
        </span>
        <button type="button" className="btn btn-primary btn-sm" disabled={answers.length === 0 || busy} onClick={() => void send()}>
          {busy ? "Отправляю…" : "Отправить ответы и перепроверить"}
        </button>
      </div>
      <div className="qscroll" data-scroller>
        <div className="qlist">
          <p className="muted small">
            Там, где ТЗ не даёт ответа, агент спрашивает. Ответы уйдут в перепроверку, пропущенные вопросы останутся открытыми в отчёте.
          </p>
          {visible.map((x) => (
            <Row key={x.q.id} item={x} draft={drafts[x.q.id]} onChange={(d) => change(x.q.id, d)} />
          ))}
          {visible.length === 0 && <div className="empty-row">Ничего не найдено.</div>}
        </div>
      </div>
    </div>
  );
}
