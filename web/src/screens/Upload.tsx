import { useMemo, useRef, useState } from "react";
import { createReview, uploadDocument } from "../api";
import { buildDoc } from "../lib/doc";
import { drop, save } from "../lib/store";
import { navigate, reviewPath } from "../route";
import { notifyError } from "../toast";
import type { Spec } from "../types";
import { DocView } from "./DocView";

const ACCEPT = [".pdf", ".md", ".txt"];
const NO_HITS = new Map();

function formatSize(bytes: number): string {
  return bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} КБ` : `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
}

export function Upload({ onCreated }: { onCreated: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [drag, setDrag] = useState(false);
  const [busy, setBusy] = useState(false);
  const [doc, setDoc] = useState<{ spec: Spec; file: File } | null>(null);
  const lines = useMemo(() => (doc ? buildDoc(doc.spec.text) : []), [doc]);

  const pick = async (file: File | undefined) => {
    if (!file) return;
    if (!ACCEPT.some((ext) => file.name.toLowerCase().endsWith(ext))) {
      notifyError(new Error("Подходят файлы .pdf, .md и .txt"));
      return;
    }
    setBusy(true);
    try {
      setDoc({ spec: await uploadDocument(file), file });
    } catch (e) {
      notifyError(e);
    } finally {
      setBusy(false);
    }
  };

  const start = async () => {
    if (!doc) return;
    setBusy(true);
    try {
      const review = await createReview(doc.spec);
      drop(`anyq:answers:${review.id}`);
      save(`anyq:started:${review.id}`, Date.now());
      onCreated();
      navigate(reviewPath(review.id));
    } catch (e) {
      notifyError(e);
      setBusy(false);
    }
  };

  return (
    <div
      className="upload"
      onDragOver={(e) => {
        e.preventDefault();
        setDrag(true);
      }}
      onDragLeave={(e) => {
        if (e.currentTarget === e.target) setDrag(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDrag(false);
        void pick(e.dataTransfer.files[0]);
      }}
    >
      <header className="page-head">
        <h1 className="page-title">Новая проверка</h1>
        <p className="muted small">
          Агент разбирает ТЗ на требования, проверяет их по ISO/IEC/IEEE 29148 и чек-листу банка, к каждому замечанию даёт цитату, а где текста не
          хватает — задаёт вопрос автору.
        </p>
      </header>

      <input
        ref={input}
        type="file"
        accept={ACCEPT.join(",")}
        hidden
        onChange={(e) => {
          void pick(e.target.files?.[0]);
          e.target.value = "";
        }}
      />

      {doc ? (
        <section className="sum-card up-file">
          <span className="file-badge">{(doc.file.name.split(".").pop() ?? "").toUpperCase().slice(0, 4)}</span>
          <div className="file-info">
            <div className="file-title">{doc.spec.title ?? doc.file.name}</div>
            <div className="file-sub mono">
              {doc.file.name} · {formatSize(doc.file.size)} · {lines.length} строк
            </div>
          </div>
          <button type="button" className="btn btn-ghost btn-sm" disabled={busy} onClick={() => input.current?.click()}>
            Заменить файл
          </button>
          <button type="button" className="btn btn-primary btn-sm" disabled={busy} onClick={() => void start()}>
            {busy ? "Запускаю…" : "Проверить ТЗ"}
          </button>
        </section>
      ) : (
        <button type="button" className={`dropzone ${drag ? "drag" : ""}`} disabled={busy} onClick={() => input.current?.click()}>
          <span className="dz-title">{busy ? "Читаю документ…" : "Перетащите файл ТЗ сюда или нажмите, чтобы выбрать"}</span>
          <span className="muted small">PDF, Markdown или текст. Проверка идёт около трёх минут.</span>
        </button>
      )}

      {doc && (
        <section className="sum-card up-preview">
          <div className="card-head">
            <span className="card-title">Распознанный текст</span>
            <span className="muted small">так его увидит агент</span>
          </div>
          <div className="up-doc">
            <DocView lines={lines} hits={NO_HITS} selected={null} onSelect={() => undefined} />
          </div>
        </section>
      )}
    </div>
  );
}
