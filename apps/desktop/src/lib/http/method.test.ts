import { expect, test } from "vitest";
import { methodColor, methodLabel } from "./method";

test("method_colours_follow_the_tokens", () => {
  for (const m of ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"])
    expect(methodColor(m)).toBe(`var(--method-${m.toLowerCase()})`);
  expect(methodColor("PURGE")).toBe("var(--text-2)");
  expect(methodLabel("PROPFIND1")).toBe("PROPFIN…");
  expect(methodLabel("GET")).toBe("GET");
});
