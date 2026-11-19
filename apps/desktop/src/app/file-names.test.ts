// @vitest-environment node
import { readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { expect, test } from "vitest";

const src = join(import.meta.dirname, "..");

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(dir, e.name)) : [join(dir, e.name)],
  );
}

// On macOS and Windows "./toasts.svelte" opens Toasts.svelte instead of
// toasts.svelte.ts, while Linux finds the right file: an import that works
// in CI breaks on a laptop.
test("no import path can mean two files on a case-insensitive disk", () => {
  const seen = new Map<string, string>();
  const clashes: string[] = [];
  for (const path of files(src)) {
    const rel = relative(src, path);
    const key = rel.replace(/\.(ts|js)$/, "").toLowerCase();
    const other = seen.get(key);
    if (other) clashes.push(`${other} and ${rel}`);
    else seen.set(key, rel);
  }
  expect(clashes).toEqual([]);
});
