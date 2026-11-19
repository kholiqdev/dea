import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

import { expect, test } from "vitest";
import { findColorLiterals } from "./color-literals";

const repo = join(import.meta.dirname, "../../..");
const ROOTS = ["apps/desktop/src", "packages/ui/src"];
const ALLOWED = new Set(["packages/ui/src/tokens.css"]);

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return files(path);
    return /\.(svelte|css|ts)$/.test(name) && !name.endsWith(".test.ts")
      ? [path]
      : [];
  });
}

test("finds hex, functional and named colours", () => {
  expect(
    findColorLiterals("a { color: #fff; background: rgba(0,0,0,.5) }"),
  ).toEqual(["#fff", "rgba("]);
  expect(
    findColorLiterals("b { border-color: red; fill: currentColor }"),
  ).toEqual(["red"]);
});

test("allows keywords and leaves non-colour text alone", () => {
  expect(
    findColorLiterals(
      "a { color: inherit; background: transparent; outline-color: currentColor }",
    ),
  ).toEqual([]);
  expect(
    findColorLiterals("{#each items as item}{#if red}{/if}{/each}"),
  ).toEqual([]);
  expect(findColorLiterals("a { color: var(--text-1); width: 12px }")).toEqual(
    [],
  );
  expect(findColorLiterals("a { background: #12345678 }")).toEqual([
    "#12345678",
  ]);
  expect(findColorLiterals("a { color: hsl(0 0% 0%) }")).toEqual(["hsl("]);
});

test("no component or stylesheet outside tokens.css has a colour literal", () => {
  const offenders = ROOTS.flatMap((root) => files(join(repo, root)))
    .map((path) => relative(repo, path))
    .filter((path) => !ALLOWED.has(path))
    .flatMap((path) =>
      findColorLiterals(readFileSync(join(repo, path), "utf8")).map(
        (literal) => `${path}: ${literal}`,
      ),
    );
  expect(offenders).toEqual([]);
});
