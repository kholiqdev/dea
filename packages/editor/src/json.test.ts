import { describe, expect, test } from "vitest";
import { formatJson, lintJson } from "./json";

describe("json", () => {
  test("json_lint_accepts_bare_variables", () => {
    expect(lintJson('{ "limit": {{limit}}, "id": "{{id}}" }')).toBeNull();
  });

  test("lint error names the line", () => {
    expect(lintJson('{\n  "a": 1,\n  "b": \n}')).toMatchObject({ line: 4 });
  });

  test("lint reports a column and a message", () => {
    const error = lintJson('{"a": }');
    expect(error?.line).toBe(1);
    expect(error?.column).toBeGreaterThan(1);
    expect(error?.message).not.toBe("");
  });

  test("lint leaves braces inside strings alone", () => {
    expect(lintJson('{"a": "{{not closed"}')).toBeNull();
  });

  test("format keeps variables and indents by two spaces", () => {
    expect(formatJson('{"a":{{n}},"b":[1,2]}')).toBe(
      '{\n  "a": {{n}},\n  "b": [\n    1,\n    2\n  ]\n}',
    );
  });

  test("format keeps variables inside strings", () => {
    expect(formatJson('{"a":"{{n}}"}')).toBe('{\n  "a": "{{n}}"\n}');
  });

  test("format returns null for invalid JSON", () => {
    expect(formatJson('{"a":')).toBeNull();
  });
});
