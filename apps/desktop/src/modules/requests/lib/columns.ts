/**
 * Widths of the Key and Description columns, in px. The shares are of the
 * width between the fixed columns: Key 35% (28% with descriptions shown),
 * Description 28%, and Value takes the rest. A table not measured yet gives
 * no widths, and the columns share the width equally.
 */
export function columnWidths(
  total: number,
  fixed: number,
  descriptions: boolean,
): { key: number; description: number } | null {
  const free = total - fixed;
  if (!(free > 0)) return null;
  return {
    key: Math.round(free * (descriptions ? 0.28 : 0.35)),
    description: descriptions ? Math.round(free * 0.28) : 0,
  };
}

/** Reports the width of `element` now and when it changes; returns the stop. */
export function watchWidth(
  element: HTMLElement,
  report: (width: number) => void,
): () => void {
  report(element.clientWidth);
  if (typeof ResizeObserver === "undefined") return () => {};
  const observer = new ResizeObserver(() => report(element.clientWidth));
  observer.observe(element);
  return () => observer.disconnect();
}
