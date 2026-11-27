import { useEffect, useState } from "react";

export type Tab = "summary" | "findings" | "document" | "questions" | "passport";
const TABS: Tab[] = ["summary", "findings", "document", "questions", "passport"];

export type Route = { name: "new" } | { name: "review"; id: string; tab: Tab };

export function parseHash(hash: string): Route {
  const [, kind, id, tab] = hash.replace(/^#/, "").split("/");
  if (kind === "r" && id) return { name: "review", id: decodeURIComponent(id), tab: TABS.find((t) => t === tab) ?? "summary" };
  return { name: "new" };
}

export function reviewPath(id: string, tab: Tab = "summary"): string {
  return `#/r/${encodeURIComponent(id)}${tab === "summary" ? "" : `/${tab}`}`;
}

export function navigate(hash: string): void {
  location.hash = hash;
}

export function useRoute(): Route {
  const [route, setRoute] = useState(() => parseHash(location.hash));
  useEffect(() => {
    const on = () => setRoute(parseHash(location.hash));
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return route;
}
