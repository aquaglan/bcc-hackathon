import { useEffect, useState } from "react";
import { getReview } from "../api";
import { navigate, reviewPath, type Tab } from "../route";
import { notifyError } from "../toast";
import type { Review } from "../types";
import { ProgressScreen } from "./ProgressScreen";
import { Result } from "./Result";

export function ReviewPage({ id, tab, onChanged }: { id: string; tab: Tab; onChanged: () => void }) {
  const [review, setReview] = useState<Review | null>(null);
  const [failed, setFailed] = useState(false);
  const [epoch, setEpoch] = useState(0);

  useEffect(() => {
    let alive = true;
    let timer = 0;
    let reported = false;
    let hadError = false;
    const tick = async () => {
      let again = true;
      try {
        const r = await getReview(id);
        if (!alive) return;
        setReview(r);
        setFailed(false);
        reported = false;
        hadError = false;
        again = r.status === "running";
        if (r.status !== "running") onChanged();
      } catch (e) {
        if (!alive) return;
        setFailed(true);
        hadError = true;
        if (!reported) notifyError(e);
        reported = true;
      }
      if (alive && again) timer = window.setTimeout(tick, hadError ? 5000 : 2500);
    };
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
    // onChanged is deliberately not a dependency: restarting polling on it would loop
  }, [id, epoch]);

  if (!review)
    return (
      <div className="center-screen">
        {failed ? (
          <>
            <p>Не удалось загрузить проверку.</p>
            <button type="button" className="btn btn-primary" onClick={() => setEpoch((n) => n + 1)}>
              Повторить
            </button>
          </>
        ) : (
          <p className="loading">Загружаю<span className="ell" /></p>
        )}
      </div>
    );

  if (review.status === "running")
    return <ProgressScreen review={review} />;

  if (review.status === "failed")
    return (
      <div className="center-screen">
        <h2>Проверка не удалась</h2>
        <p>{review.error ?? "Причина не указана."}</p>
        <button type="button" className="btn btn-primary" onClick={() => navigate("#/")}>
          Новая проверка
        </button>
      </div>
    );

  if (review.status === "awaiting_answers" || !review.report)
    return (
      <div className="center-screen">
        <h2>Агент ждёт ответов</h2>
        <p>Этот режим диалога в веб-клиенте не поддерживается.</p>
      </div>
    );

  return (
    <Result
      review={review}
      report={review.report}
      tab={tab}
      onRechecked={(r) => {
        setReview(r);
        setEpoch((n) => n + 1);
        onChanged();
        navigate(reviewPath(r.id));
      }}
    />
  );
}
