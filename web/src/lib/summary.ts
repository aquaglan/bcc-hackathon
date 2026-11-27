// The number of findings the critic refuted exists only inside the summary text.
export function parseRefuted(summary: string): number | null {
  const m = /опровергнутых критиком:\s*(\d+)/.exec(summary);
  return m ? Number(m[1]) : null;
}
