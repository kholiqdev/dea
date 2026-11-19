// WCAG 2.1 contrast, used by the token tests to keep both themes readable.

type Theme = "dark" | "light";

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;

function channels(hex: string): [number, number, number] {
  const match = HEX.exec(hex.trim());
  if (!match?.[1]) throw new Error(`not an opaque hex colour: ${hex}`);
  const digits =
    match[1].length === 3 ? [...match[1]].map((d) => d + d).join("") : match[1];
  return [0, 2, 4].map((i) => parseInt(digits.slice(i, i + 2), 16)) as [
    number,
    number,
    number,
  ];
}

function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map((c) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG 2.1 contrast ratio of two opaque hex colours, 1 to 21. */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [
    number,
    number,
  ];
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * The custom properties `theme` ends up with on the root element. Rules are
 * applied in source order, as the cascade does for selectors of equal
 * weight: `:root` always applies, `[data-theme='<theme>']` only for its
 * theme.
 */
export function readTokens(css: string, theme: Theme): Record<string, string> {
  const out: Record<string, string> = {};
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
  for (const rule of stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selectors = (rule[1] ?? "").split(",").map((s) => s.trim());
    const applies = selectors.some(
      (s) =>
        s === ":root" || s.replace(/"/g, "'") === `[data-theme='${theme}']`,
    );
    if (!applies) continue;
    for (const decl of (rule[2] ?? "").split(";")) {
      const at = decl.indexOf(":");
      if (at < 0) continue;
      const name = decl.slice(0, at).trim();
      if (name.startsWith("--")) out[name] = decl.slice(at + 1).trim();
    }
  }
  return out;
}
