import { expect, test } from "vitest";
import { variableRanges } from "./variables";

test("variable ranges skip vault and dynamic prefixes in names only", () => {
  expect(
    variableRanges("{{baseUrl}}/x/{{vault:token}}/{{$guid}}").map(
      (r) => r.name,
    ),
  ).toEqual(["baseUrl", "vault:token", "$guid"]);
});

test("variable ranges cover the braces", () => {
  const text = "a {{ id }} b";
  const range = variableRanges(text)[0]!;
  expect(text.slice(range.from, range.to)).toBe("{{ id }}");
  expect(range.name).toBe("id");
});

test("variable ranges ignore unclosed and empty placeholders", () => {
  expect(variableRanges("{{open and {{}} {{ }}")).toEqual([]);
});
