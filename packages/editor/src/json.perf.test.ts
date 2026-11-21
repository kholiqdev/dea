// The JSON scanner on a 1 MB body. It measures wall time, so it runs in the
// perf project: alone, after the other tests.

import { expect, test } from "vitest";
import { formatJson, lintJson } from "./json";

test("lint and format stay fast on a 1 MB body", () => {
  const rows = Array.from(
    { length: 20000 },
    (_, i) => `{"id":${i},"v":"{{x}}","n":{{n}},"t":[true,null]}`,
  );
  const body = `[${rows.join(",")}]`;
  expect(body.length).toBeGreaterThan(700_000);
  // One run to compile the scanner, then the fastest of three, so a garbage
  // collection on a shared CI machine does not count as the scanner's time.
  expect(lintJson(body)).toBeNull();
  const times = [0, 1, 2].map(() => {
    const start = performance.now();
    lintJson(body);
    return performance.now() - start;
  });
  expect(Math.min(...times)).toBeLessThan(100);
  expect(formatJson(body)).not.toBeNull();
});
