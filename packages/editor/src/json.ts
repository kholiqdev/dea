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

function canContinue(prefix: string): boolean {
  try {
    JSON.parse(prefix);
    return true;
  } catch (error) {
    return /end of JSON input|Unterminated|Unexpected end/i.test(String(error));
  }
}

function errorOffset(masked: string, message: string): number {
  const position = /position (\d+)/.exec(message);
  if (position) return Number(position[1]);
  const lc = /line (\d+) column (\d+)/.exec(message);
  if (lc) {
    const lines = masked.split("\n");
    let offset = 0;
    for (let i = 0; i < Number(lc[1]) - 1; i++)
      offset += (lines[i] ?? "").length + 1;
    return offset + Number(lc[2]) - 1;
  }
  if (/end of JSON input/i.test(message)) return masked.length;
  const token =
    /Unexpected (?:token|non-whitespace character after JSON at position) ?'?(.)'?/.exec(
      message,
    );
  if (token) {
    for (let i = 0; i < masked.length; i++) {
      if (
        masked[i] === token[1] &&
        canContinue(masked.slice(0, i)) &&
        !canContinue(masked.slice(0, i + 1))
      ) {
        return i;
      }
    }
  }
  for (let i = 0; i <= masked.length; i++) {
    if (!canContinue(masked.slice(0, i))) return Math.max(0, i - 1);
  }
  return masked.length;
}

export function lintJson(text: string): JsonLintError | null {
  const masked = maskVariables(text);
  try {
    JSON.parse(masked);
    return null;
  } catch (error) {
    const raw = error instanceof Error ? error.message : String(error);
    const offset = Math.min(errorOffset(masked, raw), text.length);
    return {
      ...lineColumn(text, offset),
      message: raw
        .replace(/,\s*(?:\.\.\.)?".*"\s*is not valid JSON$/s, "")
        .replace(/\s+in JSON at position.*$/, ""),
    };
  }
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
      const next = text.slice(i + 1).search(/\S/);
      if (next !== -1 && text[i + 1 + next] === close) {
        out += c + close;
        i += 1 + next;
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
