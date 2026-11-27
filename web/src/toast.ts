import { useEffect, useState } from "react";

type Listener = (message: string | null) => void;
const listeners = new Set<Listener>();

export function notifyError(e: unknown): void {
  const message = e instanceof Error ? e.message : "Что-то пошло не так";
  listeners.forEach((l) => l(message));
}

export function useToast(): [string | null, () => void] {
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => {
    listeners.add(setMessage);
    return () => void listeners.delete(setMessage);
  }, []);
  useEffect(() => {
    if (!message) return;
    const t = window.setTimeout(() => setMessage(null), 8000);
    return () => window.clearTimeout(t);
  }, [message]);
  return [message, () => setMessage(null)];
}
