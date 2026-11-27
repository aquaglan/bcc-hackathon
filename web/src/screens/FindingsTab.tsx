import { useEffect, useMemo, useRef, useState } from "react";
import {
  matches,
  NO_FILTER,
  sortRows,
  STATUS_LABEL,
  type Decision,
  type Filter,
  type Sort,
  type Status,
  type StatusFilter,
  type Verified,
} from "../lib/decisions";
import type { DocLine } from "../lib/doc";
import { SEV_LABEL } from "../lib/readiness";
import { load, save } from "../lib/store";
import type { Severity } from "../types";
import { DocView, hitsByLine } from "./DocView";
import { Chevron, Tick } from "./icons";
import type { FindingItem } from "./model";

interface Props {
  reviewId: string;
  lines: DocLine[];
  items: FindingItem[];
  selected: number | null;
  onSelect: (i: number) => void;
  onDecide: (key: string, d: Decision) => void;
}

const SEVERITIES: Severity[] = ["critical", "major", "minor"];

export function FindingsTab({ reviewId, lines, items, selected, onSelect, onDecide }: Props) {
  const viewKey = `anyq:findings-view:${reviewId}`;
  const [filter, setFilter] = useState<Filter>(() => load(viewKey, { filter: NO_FILTER, sort: "severity" as Sort }).filter);
  const [sort, setSort] = useState<Sort>(() => load(viewKey, { filter: NO_FILTER, sort: "severity" as Sort }).sort);
  useEffect(() => save(viewKey, { filter, sort }), [viewKey, filter, sort]);

  const categories = useMemo(() => [...new Set(items.map((x) => x.f.category))].sort(), [items]);
  const shown = useMemo(() => sortRows(items.filter((x) => matches(x, filter)), sort), [items, filter, sort]);
  const hits = useMemo(() => hitsByLine(lines, items), [lines, items]);
  const cur = items.find((x) => x.i === selected) ?? shown[0] ?? null;
  const pos = cur ? shown.indexOf(cur) : -1;

  const docRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!cur || !docRef.current) return;
    const target =
      docRef.current.querySelector(`[data-f="${cur.i}"]`) ?? (cur.line ? docRef.current.querySelector(`#L${cur.line.global}`) : null);
    target?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [cur?.i]); // scroll on selection only, not on decisions

  const set = (patch: Partial<Filter>) => setFilter((f) => ({ ...f, ...patch }));
  const toggleSev = (s: Severity) =>
    set({ severities: filter.severities.includes(s) ? filter.severities.filter((x) => x !== s) : [...filter.severities, s] });
  const goto = (d: number) => {
    if (shown.length === 0) return;
    const next = shown[(pos + d + shown.length) % shown.length];
    if (next) onSelect(next.i);
  };
  const filtered = filter.q !== "" || filter.severities.length > 0 || filter.status !== "all" || filter.category !== "" || filter.verified !== "all";

  return (
    <div className="ft">
      <aside className="ft-list">
        <div className="ft-tools">
          <input
            className="input"
            type="search"
            placeholder="Поиск по замечаниям"
            value={filter.q}
            onChange={(e) => set({ q: e.target.value })}
            aria-label="Поиск по замечаниям"
          />
          <div className="ft-sev" role="group" aria-label="Важность">
            {SEVERITIES.map((s) => (
              <button
                type="button"
                key={s}
                className={`chip ${filter.severities.includes(s) ? "on" : ""}`}
                aria-pressed={filter.severities.includes(s)}
                onClick={() => toggleSev(s)}
              >
                <span className="dot" style={{ background: `var(--sev-${s})` }} />
                {SEV_LABEL[s]} <span className="muted">{items.filter((x) => x.f.severity === s).length}</span>
              </button>
            ))}
          </div>
          <div className="ft-selects">
            <select className="input" value={filter.status} onChange={(e) => set({ status: e.target.value as StatusFilter })} aria-label="Статус">
              <option value="all">Все статусы</option>
              <option value="open">Открытые</option>
              <option value="closed">Закрытые</option>
              <option value="accepted">Принятые</option>
              <option value="edited">Со своей правкой</option>
              <option value="rejected">Отклонённые</option>
            </select>
            <select className="input" value={filter.category} onChange={(e) => set({ category: e.target.value })} aria-label="Категория">
              <option value="">Все категории</option>
              {categories.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
            <select className="input" value={filter.verified} onChange={(e) => set({ verified: e.target.value as Verified })} aria-label="Критик">
              <option value="all">Критик: все</option>
              <option value="yes">Подтверждены</option>
              <option value="no">Не проверены</option>
            </select>
            <select className="input" value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="Сортировка">
              <option value="severity">По важности</option>
              <option value="document">По порядку в документе</option>
              <option value="category">По категории</option>
              <option value="status">По статусу</option>
            </select>
          </div>
          <div className="ft-count">
            <span>
              {shown.length} из {items.length}
            </span>
            {filtered && (
              <button type="button" className="link-btn" onClick={() => setFilter(NO_FILTER)}>
                Сбросить фильтры
              </button>
            )}
          </div>
        </div>
        <ul className="fl">
          {shown.map((x) => (
            <li key={x.i}>
              <button type="button" className={`fl-row ${x.i === cur?.i ? "on" : ""} ${x.closed ? "done" : ""}`} onClick={() => onSelect(x.i)}>
                <span className="dot" style={{ background: `var(--sev-${x.f.severity})` }} />
                <span className="fl-body">
                  <span className="fl-text">{x.f.problem}</span>
                  <span className="fl-meta">
                    <span className="mono">{x.f.category}</span>
                    {x.decision.status !== "open" && <span className={`st st-${x.decision.status}`}>{STATUS_LABEL[x.decision.status]}</span>}
                    {x.decision.comment && <span className="fl-note" title={x.decision.comment}>комментарий</span>}
                  </span>
                </span>
              </button>
            </li>
          ))}
          {shown.length === 0 && <li className="fl-empty">Ничего не найдено.</li>}
        </ul>
      </aside>

      <div className="ft-doc" ref={docRef}>
        <DocView lines={lines} hits={hits} selected={cur?.i ?? null} onSelect={onSelect} />
      </div>

      <aside className="ft-detail">
        {cur ? (
          <Detail key={cur.i} item={cur} pos={pos} total={shown.length} onGo={goto} onDecide={(d) => onDecide(cur.key, d)} />
        ) : (
          <p className="muted">Выберите замечание слева.</p>
        )}
      </aside>
    </div>
  );
}

function Detail({ item, pos, total, onGo, onDecide }: { item: FindingItem; pos: number; total: number; onGo: (d: number) => void; onDecide: (d: Decision) => void }) {
  const d = item.decision;
  const [editing, setEditing] = useState(false);
  const [fix, setFix] = useState(d.fix ?? item.f.suggestion);
  const [comment, setComment] = useState(d.comment ?? "");
  const saveComment = () => {
    const c = comment.trim() || null;
    if (c !== d.comment) onDecide({ ...d, comment: c });
  };

  return (
    <div className="fd">
      <div className="fd-head">
        <span className={`sev-chip sev-${item.f.severity}`}>{SEV_LABEL[item.f.severity]}</span>
        <span className="mono fd-cat">{item.f.category}</span>
        <span className="fd-pos">
          <button type="button" className="icon-btn" aria-label="Предыдущее замечание" onClick={() => onGo(-1)}>
            <Chevron dir="left" />
          </button>
          {pos >= 0 ? `${pos + 1} из ${total}` : "вне фильтра"}
          <button type="button" className="icon-btn" aria-label="Следующее замечание" onClick={() => onGo(1)}>
            <Chevron dir="right" />
          </button>
        </span>
      </div>
      {item.f.section && <div className="fd-place">{item.f.section}</div>}

      <div className="fd-label">Проблема</div>
      <p className="fd-problem">{item.f.problem}</p>

      <div className="fd-label">Было в ТЗ</div>
      {item.f.quote ? (
        <div className="fix-old">
          {item.f.quote}
          {!item.range && <div className="fix-warn">Фрагмент не найден в тексте, подсветки нет.</div>}
        </div>
      ) : (
        <div className="fix-old none">В документе этого нет.</div>
      )}

      <div className="fd-label">
        {d.status === "edited" ? "Своя правка" : "Предлагает агент"}
        {!editing && (
          <button type="button" className="link-btn push" onClick={() => setEditing(true)}>
            {d.status === "edited" ? "Изменить" : "Написать свою"}
          </button>
        )}
      </div>
      {editing ? (
        <div className="fd-edit">
          <textarea className="input" rows={5} value={fix} onChange={(e) => setFix(e.target.value)} autoFocus />
          <div className="fd-row">
            <button
              type="button"
              className="btn btn-primary btn-sm"
              disabled={!fix.trim()}
              onClick={() => {
                onDecide({ ...d, status: "edited", fix: fix.trim() });
                setEditing(false);
              }}
            >
              Сохранить правку
            </button>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setEditing(false)}>
              Отмена
            </button>
          </div>
        </div>
      ) : (
        <div className="fix-new">{d.status === "edited" && d.fix ? d.fix : item.f.suggestion}</div>
      )}

      <div className={`critic ${item.f.verified ? "ok" : ""}`}>
        <div className="critic-title">
          {item.f.verified && <Tick />}
          {item.f.verified ? "Подтверждено критиком" : "Критик не проверял"}
        </div>
        {item.f.critic_note && <p>{item.f.critic_note}</p>}
      </div>

      <div className="fd-label">Решение</div>
      <div className="seg" role="group" aria-label="Решение">
        {(["open", "accepted", "edited", "rejected"] as Status[]).map((s) => (
          <button
            type="button"
            key={s}
            className={`seg-btn st-${s} ${d.status === s ? "on" : ""}`}
            aria-pressed={d.status === s}
            disabled={s === "edited" && !d.fix}
            title={s === "edited" && !d.fix ? "Сначала напишите свою правку" : undefined}
            onClick={() => onDecide({ ...d, status: s })}
          >
            {STATUS_LABEL[s]}
          </button>
        ))}
      </div>

      <div className="fd-label">Комментарий</div>
      <textarea
        className="input"
        rows={3}
        placeholder={d.status === "rejected" ? "Почему отклонено" : "Заметка для себя или автора ТЗ"}
        value={comment}
        onChange={(e) => setComment(e.target.value)}
        onBlur={saveComment}
      />
      {comment.trim() !== (d.comment ?? "") && (
        <div className="fd-row">
          <button type="button" className="btn btn-primary btn-sm" onClick={saveComment}>
            Сохранить комментарий
          </button>
        </div>
      )}
      <p className="fd-hint">Решения хранятся в этом браузере. Сам документ не меняется.</p>
    </div>
  );
}
