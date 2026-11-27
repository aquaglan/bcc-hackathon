// Storage can be blocked or full; the app must work without it.
export function load<T>(key: string, fallback: T): T {
  try {
    const s = localStorage.getItem(key);
    return s === null ? fallback : (JSON.parse(s) as T);
  } catch {
    return fallback;
  }
}

export function save(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // losing a draft is better than breaking the screen
  }
}

export function drop(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // see save
  }
}
