// The few drawn icons the app needs, one stroke weight.
export function Chevron({ dir }: { dir: "left" | "right" }) {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={dir === "left" ? "M15 5l-7 7 7 7" : "M9 5l7 7-7 7"} />
    </svg>
  );
}

// The check draws itself when it appears (CSS: .tick path).
export function Tick() {
  return (
    <span className="tick" aria-hidden>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round">
        <path d="M5 12.5l4.5 4.5L19 7.5" pathLength={1} />
      </svg>
    </span>
  );
}
