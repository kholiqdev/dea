import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import { contrastRatio, readTokens } from "./contrast";

const tokensCss = readFileSync(join(import.meta.dirname, "tokens.css"), "utf8");

// Pairs every component may use, in both themes.
const TEXT = [
  "--text-1",
  "--text-2",
  "--text-3",
  "--negative-text",
  "--positive-text",
  "--warning-text",
  "--method-get",
  "--method-post",
  "--method-put",
  "--method-patch",
  "--method-delete",
  "--method-head",
  "--method-options",
];
const NON_TEXT = ["--focus", "--accent", "--negative", "--positive"];
const SURFACES = ["--bg-0", "--bg-1", "--bg-2", "--bg-3", "--bg-hover"];

function token(t: Record<string, string>, name: string): string {
  const value = t[name];
  if (value === undefined) throw new Error(`missing token ${name}`);
  return value;
}

test.each(["dark", "light"] as const)(
  "text on every surface is at least 4.5:1 in %s",
  (theme) => {
    const t = readTokens(tokensCss, theme);
    for (const fg of TEXT)
      for (const bg of SURFACES)
        expect(
          contrastRatio(token(t, fg), token(t, bg)),
          `${fg} on ${bg}`,
        ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(token(t, "--primary-text"), token(t, "--primary-bg")),
    ).toBeGreaterThanOrEqual(4.5);
  },
);

test.each(["dark", "light"] as const)(
  "indicators are at least 3:1 in %s",
  (theme) => {
    const t = readTokens(tokensCss, theme);
    for (const fg of NON_TEXT)
      for (const bg of SURFACES)
        expect(
          contrastRatio(token(t, fg), token(t, bg)),
          `${fg} on ${bg}`,
        ).toBeGreaterThanOrEqual(3);
    expect(
      contrastRatio(token(t, "--on-accent"), token(t, "--accent")),
    ).toBeGreaterThanOrEqual(3);
  },
);

test("light theme overrides accent and positive", () => {
  const t = readTokens(tokensCss, "light");
  expect(t["--accent"]).toBe("#a86a12");
  expect(t["--positive"]).toBe("#15803d");
});

test.each(["dark", "light"] as const)(
  "the preview canvas is white in %s, as browsers draw plain HTML",
  (theme) => {
    expect(token(readTokens(tokensCss, theme), "--preview-canvas")).toBe(
      "#ffffff",
    );
  },
);

test("the light theme inherits what it does not set from the root", () => {
  const css = `:root, [data-theme='dark'] { --a: #000000; --b: #111111 }
[data-theme='light'] { --a: #ffffff }`;
  expect(readTokens(css, "light")).toEqual({
    "--a": "#ffffff",
    "--b": "#111111",
  });
  expect(readTokens(css, "dark")).toEqual({
    "--a": "#000000",
    "--b": "#111111",
  });
});

test("contrast follows WCAG 2.1", () => {
  expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
  expect(contrastRatio("#fff", "#fff")).toBeCloseTo(1, 5);
  expect(contrastRatio("#777777", "#ffffff")).toBeCloseTo(4.48, 2);
});

const ENV_COLORS = [
  "--env-slate",
  "--env-blue",
  "--env-green",
  "--env-amber",
  "--env-silver",
];

test.each(["dark", "light"] as const)(
  "environment colours stay off red and readable in %s",
  (theme) => {
    const t = readTokens(tokensCss, theme);
    for (const c of ENV_COLORS) {
      expect(token(t, c), c).not.toBe(token(t, "--negative"));
      for (const bg of SURFACES)
        expect(
          contrastRatio(token(t, c), token(t, bg)),
          `${c} on ${bg}`,
        ).toBeGreaterThanOrEqual(3);
    }
  },
);
