import { variableRanges } from "./variables";

export interface JsonLintError {
  line: number;
  column: number;
  message: string;
}

/** Replaces each `{{variable}}` outside strings with a number literal of the same length. */
function maskVariables(text: string): string {
  const chars = text.split("");
  const outside = outsideStrings(text);
  for (const { from, to } of variableRanges(text)) {
    if (!outside[from]) continue;
    for (let i = from; i < to; i++) chars[i] = "1";
  }
  return chars.join("");
}

/** Marks which offsets are outside a JSON string. */
function outsideStrings(text: string): boolean[] {
  const outside: boolean[] = [];
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inString) {
      outside.push(false);
      if (c === "\\") {
        i++;
        outside.push(false);
      } else if (c === '"') inString = false;
    } else {
      outside.push(true);
      if (c === '"') {
        inString = true;
        outside[i] = false;
      }
    }
  }
  return outside;
}

function lineColumn(
  text: string,
  offset: number,
): { line: number; column: number } {
  const before = text.slice(0, offset);
  const line = before.split("\n").length;
  return { line, column: offset - (before.lastIndexOf("\n") + 1) + 1 };
}

interface ScanError {
  offset: number;
  message: string;
}

const WHITESPACE = new Set([" ", "\t", "\n", "\r"]);
const LITERALS = ["true", "false", "null"];

function isDigit(c: string | undefined): boolean {
  return c !== undefined && c >= "0" && c <= "9";
}

/** Returns the offset after the string starting at `start`, or an error. */
function scanString(text: string, start: number): number | ScanError {
  const unterminated = { offset: start, message: "Unterminated string" };
  for (let i = start + 1; i < text.length; i++) {
    const c = text[i] as string;
    if (c === '"') return i + 1;
    if (c < " ") return unterminated;
    if (c !== "\\") continue;
    i++;
    const escape = text[i];
    if (escape === undefined) return unterminated;
    if (escape === "u") {
      for (let k = 1; k <= 4; k++) {
        if (!/[0-9a-fA-F]/.test(text[i + k] ?? "")) {
          return {
            offset: Math.min(i + k, text.length),
            message: "Invalid escape sequence",
          };
        }
      }
      i += 4;
    } else if (!'"\\/bfnrt'.includes(escape)) {
      return { offset: i, message: "Invalid escape sequence" };
    }
  }
  return unterminated;
}

function scanNumber(text: string, start: number): number | ScanError {
  let i = start;
  const bad = (): ScanError => ({ offset: i, message: "Invalid number" });
  if (text[i] === "-") i++;
  if (text[i] === "0") i++;
  else if (isDigit(text[i])) while (isDigit(text[i])) i++;
  else return bad();
  if (text[i] === ".") {
    i++;
    if (!isDigit(text[i])) return bad();
    while (isDigit(text[i])) i++;
  }
  if (text[i] === "e" || text[i] === "E") {
    i++;
    if (text[i] === "+" || text[i] === "-") i++;
    if (!isDigit(text[i])) return bad();
    while (isDigit(text[i])) i++;
  }
  return i;
}

type Expect =
  "value" | "valueOrClose" | "keyOrClose" | "key" | "colon" | "after";

/** Linear JSON scanner: finds the first invalid offset, or null when the text is valid. */
function scanJson(text: string): ScanError | null {
  const stack: string[] = [];
  let expect: Expect = "value";
  let i = 0;
  const unexpected = (): ScanError => ({
    offset: i,
    message: `Unexpected character "${text[i]}"`,
  });
  for (;;) {
    while (i < text.length && WHITESPACE.has(text[i] as string)) i++;
    if (i >= text.length) {
      return expect === "after" && stack.length === 0
        ? null
        : { offset: text.length, message: "Unexpected end of input" };
    }
    const c = text[i] as string;
    const top = stack[stack.length - 1];
    switch (expect) {
      case "valueOrClose":
      case "value": {
        if (expect === "valueOrClose" && c === "]") {
          stack.pop();
          expect = "after";
          i++;
        } else if (c === "{" || c === "[") {
          stack.push(c);
          expect = c === "{" ? "keyOrClose" : "valueOrClose";
          i++;
        } else if (c === '"' || c === "-" || isDigit(c)) {
          const end = c === '"' ? scanString(text, i) : scanNumber(text, i);
          if (typeof end !== "number") return end;
          i = end;
          expect = "after";
        } else {
          const literal = LITERALS.find((l) => text.startsWith(l, i));
          if (!literal) return unexpected();
          i += literal.length;
          expect = "after";
        }
        break;
      }
      case "keyOrClose":
      case "key": {
        if (expect === "keyOrClose" && c === "}") {
          stack.pop();
          expect = "after";
          i++;
        } else if (c === '"') {
          const end = scanString(text, i);
          if (typeof end !== "number") return end;
          i = end;
          expect = "colon";
        } else {
          return {
            offset: i,
            message:
              expect === "key"
                ? "Expected a property name"
                : 'Expected a property name or "}"',
          };
        }
        break;
      }
      case "colon": {
        if (c !== ":") return { offset: i, message: 'Expected ":"' };
        i++;
        expect = "value";
        break;
      }
      case "after": {
        if (top === undefined) return unexpected();
        const close = top === "{" ? "}" : "]";
        if (c === ",") {
          expect = top === "{" ? "key" : "value";
          i++;
        } else if (c === close) {
          stack.pop();
          i++;
        } else {
          return { offset: i, message: `Expected "," or "${close}"` };
        }
        break;
      }
    }
  }
}

export function lintJson(text: string): JsonLintError | null {
  // Nothing typed yet is "no body", not a mistake.
  if (text.trim() === "") return null;
  const error = scanJson(maskVariables(text));
  if (!error) return null;
  return { ...lineColumn(text, error.offset), message: error.message };
}

/** Pretty-prints with two spaces, keeping variables and number literals as written. */
export function formatJson(text: string): string | null {
  if (lintJson(text) !== null) return null;
  const outside = outsideStrings(text);
  const ranges = new Map(
    variableRanges(text)
      .filter((r) => outside[r.from])
      .map((r) => [r.from, r.to]),
  );
  let out = "";
  let depth = 0;
  const indent = () => "\n" + "  ".repeat(depth);
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    const variableEnd = ranges.get(i);
    if (variableEnd !== undefined) {
      out += text.slice(i, variableEnd);
      i = variableEnd - 1;
    } else if (c === '"') {
      let end = i + 1;
      while (text[end] !== '"') end += text[end] === "\\" ? 2 : 1;
      out += text.slice(i, end + 1);
      i = end;
    } else if (c === "{" || c === "[") {
      const close = c === "{" ? "}" : "]";
      let next = i + 1;
      while (WHITESPACE.has(text[next] as string)) next++;
      if (text[next] === close) {
        out += c + close;
        i = next;
      } else {
        depth++;
        out += c + indent();
      }
    } else if (c === "}" || c === "]") {
      depth--;
      out += indent() + c;
    } else if (c === ",") out += "," + indent();
    else if (c === ":") out += ": ";
    else if (c !== undefined && !/\s/.test(c)) out += c;
  }
  return out;
}
