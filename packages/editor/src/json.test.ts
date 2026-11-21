import { describe, expect, test, vi } from "vitest";
import { formatJson, lintJson } from "./json";

describe("json", () => {
  test("json_lint_accepts_bare_variables", () => {
    expect(lintJson('{ "limit": {{limit}}, "id": "{{id}}" }')).toBeNull();
  });

  test("lint treats an empty or whitespace-only body as no body", () => {
    expect(lintJson("")).toBeNull();
    expect(lintJson(" \n\t ")).toBeNull();
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

  test("lint never calls JSON.parse, so engine messages cannot matter", () => {
    const parse = vi.spyOn(JSON, "parse").mockImplementation(() => {
      throw new SyntaxError("JSON Parse error: Unexpected EOF");
    });
    try {
      expect(lintJson('{\n  "a": 1,\n  "b": \n}')).toMatchObject({ line: 4 });
      expect(lintJson('{"a": 1}')).toBeNull();
      expect(parse).not.toHaveBeenCalled();
    } finally {
      parse.mockRestore();
    }
  });

  test("lint finds an error on a later line after a variable", () => {
    expect(lintJson('{\n  "a": {{one}},\n  "b": 2,\n  "c" 3\n}')).toMatchObject(
      {
        line: 4,
        message: 'Expected ":"',
      },
    );
  });

  test("lint reports an unterminated string", () => {
    expect(lintJson('{\n  "a": "open\n}')).toMatchObject({
      line: 2,
      message: "Unterminated string",
    });
  });

  test("lint reports end of input", () => {
    expect(lintJson('{"a": [1, 2')).toMatchObject({
      line: 1,
      message: "Unexpected end of input",
    });
  });

  test("lint rejects a trailing comma", () => {
    expect(lintJson('{\n  "a": [1, 2,],\n  "b": 1\n}')).toMatchObject({
      line: 2,
    });
    expect(lintJson('{"a": 1,}')).toMatchObject({ line: 1 });
  });

  test("lint rejects trailing content", () => {
    expect(lintJson('{"a": 1} x')).toMatchObject({
      message: 'Unexpected character "x"',
    });
  });

  test("lint accepts escaped quotes and unicode escapes in strings", () => {
    expect(lintJson('{"a": "say \\"hi\\" \\u00e9 \\\\"}')).toBeNull();
  });

  test("lint follows the number grammar", () => {
    expect(lintJson("[0, -1, 1.5e+3, 2E-2]")).toBeNull();
    expect(lintJson("[01]")).not.toBeNull();
    expect(lintJson("[1.]")).not.toBeNull();
  });
});
