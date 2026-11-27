// The motion system: one curve (expo.out), masked text rises, blocks rise 16-40px, everything once.
// Every GSAP animation is registered under MOTION_OK, so reduced motion gets a static, fully visible screen.
import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { SplitText } from "gsap/SplitText";
import { useRef, type ElementType, type ReactNode } from "react";

gsap.registerPlugin(ScrollTrigger, SplitText, useGSAP);
export { gsap, useGSAP };

export const MOTION_OK = "(prefers-reduced-motion: no-preference)";
const EASE = "expo.out";

// Screens scroll inside their own panes, not the document; triggers must watch that pane.
function scrollerOf(el: Element): Element | undefined {
  return el.closest("[data-scroller]") ?? undefined;
}

/** Lines (or letters) rise from under a mask, once. Splits only after the web font is in. */
export function SplitHeading({
  as: Tag = "h1",
  children,
  className,
  chars,
  delay = 0,
}: {
  as?: ElementType;
  children: ReactNode;
  className?: string;
  chars?: boolean;
  delay?: number;
}) {
  const ref = useRef<HTMLElement>(null);
  useGSAP(
    () => {
      const el = ref.current;
      if (!el) return;
      const mm = gsap.matchMedia();
      mm.add(MOTION_OK, () => {
        let split: SplitText | undefined;
        let alive = true;
        gsap.set(el, { autoAlpha: 0 });
        void document.fonts.ready.then(() => {
          if (!alive) return;
          gsap.set(el, { autoAlpha: 1 });
          split = SplitText.create(el, {
            type: chars ? "lines,chars" : "lines",
            mask: "lines",
            linesClass: "split-line",
            autoSplit: true,
            onSplit: (s) =>
              gsap.from(chars ? s.chars : s.lines, {
                yPercent: 110,
                duration: 1.1,
                ease: EASE,
                stagger: chars ? 0.022 : 0.09,
                delay,
              }),
          });
        });
        return () => {
          alive = false;
          split?.revert();
        };
      });
      return () => mm.revert();
    },
    { scope: ref },
  );
  return (
    <Tag ref={ref} className={className}>
      {children}
    </Tag>
  );
}

/** Its direct children rise one after another: on mount, or when it scrolls into its pane. */
export function Reveal({
  children,
  className,
  stagger = 0.07,
  y = 24,
  delay = 0,
  onScroll,
  as: Tag = "div",
}: {
  children: ReactNode;
  className?: string;
  stagger?: number;
  y?: number;
  delay?: number;
  onScroll?: boolean;
  as?: ElementType;
}) {
  const ref = useRef<HTMLElement>(null);
  useGSAP(
    () => {
      const el = ref.current;
      if (!el) return;
      const mm = gsap.matchMedia();
      mm.add(MOTION_OK, () => {
        gsap.from(el.children, {
          y,
          autoAlpha: 0,
          duration: 1,
          ease: EASE,
          stagger,
          delay,
          scrollTrigger: onScroll ? { trigger: el, scroller: scrollerOf(el), start: "top 92%", once: true } : undefined,
        });
      });
      return () => mm.revert();
    },
    { scope: ref },
  );
  return (
    <Tag ref={ref} className={className}>
      {children}
    </Tag>
  );
}

/** A handwritten note: written left to right, then its arrow draws. One per screen at most. */
export function Scribble({
  children,
  className = "",
  delay = 0.6,
  arrow = true,
}: {
  children: ReactNode;
  className?: string;
  delay?: number;
  arrow?: boolean;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  useGSAP(
    () => {
      const mm = gsap.matchMedia();
      mm.add(MOTION_OK, () => {
        gsap
          .timeline({ delay })
          .fromTo("[data-scribble-text]", { clipPath: "inset(0 100% 0 0)" }, { clipPath: "inset(0 0% 0 0)", duration: 0.9, ease: "power2.inOut" })
          .from("path", { strokeDashoffset: 1, duration: 0.5, ease: "power2.out", stagger: 0.15 }, "-=0.2");
      });
      return () => mm.revert();
    },
    { scope: ref },
  );
  return (
    <span ref={ref} aria-hidden className={`scribble ${className}`}>
      <span data-scribble-text>{children}</span>
      {arrow && (
        <svg viewBox="0 0 40 44" fill="none" className="scribble-arrow">
          <path d="M28 2c4 10 3 22-6 30-4 3.6-9 5.6-15 6.4" pathLength={1} />
          <path d="M12 33.5 6.6 38.6l6.8 2.6" pathLength={1} />
        </svg>
      )}
    </span>
  );
}

/** Hover on the parent: the label slides up and a copy slides in from below. On every button. */
export function Roll({ children }: { children: ReactNode }) {
  return (
    <span className="roll">
      <span className="roll-a">{children}</span>
      <span className="roll-b" aria-hidden>
        {children}
      </span>
    </span>
  );
}

/** The round chip with an arrow that turns -45° when its button is hovered. */
export function ArrowChip() {
  return (
    <span className="chip-arrow" aria-hidden>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M5 12h14M13 6l6 6-6 6" />
      </svg>
    </span>
  );
}

/** A number that counts to its new value instead of jumping. */
export function Count({ value, className }: { value: number; className?: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const shown = useRef(0);
  useGSAP(
    () => {
      const el = ref.current;
      if (!el) return;
      const mm = gsap.matchMedia();
      mm.add(MOTION_OK, () => {
        const o = { v: shown.current };
        gsap.to(o, {
          v: value,
          duration: 1.2,
          ease: EASE,
          onUpdate: () => {
            shown.current = Math.round(o.v);
            el.textContent = String(shown.current);
          },
        });
      });
      mm.add("(prefers-reduced-motion: reduce)", () => {
        shown.current = value;
        el.textContent = String(value);
      });
      return () => mm.revert();
    },
    { dependencies: [value] },
  );
  return (
    <span ref={ref} className={className}>
      {value}
    </span>
  );
}
