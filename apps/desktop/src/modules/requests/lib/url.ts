import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
import type { Variable } from "$lib/ipc/bindings/Variable";

// A `{{…}}` variable holds anything except braces. Separators inside one do not count.
const VARIABLE = /\{\{[^{}]*\}\}/g;

type Pair = { name: string; value: string };

/** Index of the first `char` outside any `{{…}}`, or -1. */
function indexOutside(text: string, char: string, from = 0): number {
  for (let i = from; i < text.length; i++) {
    if (text.startsWith("{{", i)) {
      const close = /^\{\{[^{}]*\}\}/.exec(text.slice(i));
      if (close) {
        i += close[0].length - 1;
        continue;
      }
    }
    if (text[i] === char) return i;
  }
  return -1;
}

function splitOutside(text: string, char: string): string[] {
  const parts: string[] = [];
  let start = 0;
  for (
    let at = indexOutside(text, char);
    at !== -1;
    at = indexOutside(text, char, start)
  ) {
    parts.push(text.slice(start, at));
    start = at + 1;
  }
  parts.push(text.slice(start));
  return parts;
}

/** Splits the URL bar text at the first `?` outside a variable. Nothing is decoded. */
export function splitUrl(text: string): { base: string; query: Pair[] } {
  const at = indexOutside(text, "?");
  if (at === -1) return { base: text, query: [] };
  const query: Pair[] = [];
  for (const part of splitOutside(text.slice(at + 1), "&")) {
    if (part === "") continue;
    const eq = indexOutside(part, "=");
    query.push(
      eq === -1
        ? { name: part, value: "" }
        : { name: part.slice(0, eq), value: part.slice(eq + 1) },
    );
  }
  return { base: text.slice(0, at), query };
}

/** Escapes only what would change the URL's structure; variables are left alone. */
function escapeOutside(text: string, escapes: Record<string, string>): string {
  let out = "";
  let last = 0;
  for (const match of text.matchAll(VARIABLE)) {
    out += escapeChars(text.slice(last, match.index), escapes) + match[0];
    last = match.index + match[0].length;
  }
  return out + escapeChars(text.slice(last), escapes);
}

function escapeChars(text: string, escapes: Record<string, string>): string {
  let out = "";
  for (const char of text) out += escapes[char] ?? char;
  return out;
}

const NAME_ESCAPES = { "&": "%26", "#": "%23", "=": "%3D" };
const VALUE_ESCAPES = { "&": "%26", "#": "%23" };

/** Appends the enabled params to `base`. A bare name is written as `name=`. */
export function joinUrl(base: string, params: KeyValue[]): string {
  const pairs = params
    .filter((p) => p.enabled !== false)
    .map(
      (p) =>
        `${escapeOutside(p.name, NAME_ESCAPES)}=${escapeOutside(p.value, VALUE_ESCAPES)}`,
    );
  return pairs.length === 0 ? base : `${base}?${pairs.join("&")}`;
}

/**
 * Enabled params follow `typed` in order. A description survives when the name at the
 * same position is unchanged. Disabled params keep their position among the others.
 */
export function syncParams(old: KeyValue[], typed: Pair[]): KeyValue[] {
  const oldActive = old.filter((p) => p.enabled !== false);
  const disabled: { param: KeyValue; after: number }[] = [];
  let seen = 0;
  for (const param of old) {
    if (param.enabled === false) disabled.push({ param, after: seen });
    else seen++;
  }

  const result: KeyValue[] = [];
  let next = 0;
  const flush = (upTo: number) => {
    while (next < disabled.length && disabled[next]!.after <= upTo) {
      result.push(disabled[next]!.param);
      next++;
    }
  };
  typed.forEach((pair, i) => {
    flush(i);
    const description =
      oldActive[i]?.name === pair.name ? oldActive[i]?.description : undefined;
    result.push(
      description === undefined
        ? { name: pair.name, value: pair.value }
        : { name: pair.name, value: pair.value, description },
    );
  });
  flush(Infinity);
  return result;
}

const SCHEME_AUTHORITY = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//;

/** Where the path starts: after `scheme://host[:port]`, after a leading variable, or at the first `/`. */
function pathStart(url: string): number {
  const scheme = SCHEME_AUTHORITY.exec(url);
  if (scheme) {
    let i = scheme[0].length;
    while (i < url.length) {
      const variable = /^\{\{[^{}]*\}\}/.exec(url.slice(i));
      if (variable) i += variable[0].length;
      else if ("/?#".includes(url[i]!)) break;
      else i++;
    }
    return i;
  }
  if (url.startsWith("{{")) {
    const variable = /^\{\{[^{}]*\}\}/.exec(url);
    if (variable) return variable[0].length;
  }
  const slash = indexOutside(url, "/");
  return slash === -1 ? url.length : slash;
}

/** Names of `:name` segments and `{{name}}` variables in the path, in order, without duplicates. */
export function pathVariableNames(url: string): string[] {
  let path = url.slice(pathStart(url));
  const end = [indexOutside(path, "?"), indexOutside(path, "#")].filter(
    (i) => i !== -1,
  );
  if (end.length > 0) path = path.slice(0, Math.min(...end));

  const names: string[] = [];
  const add = (name: string) => {
    if (name !== "" && !names.includes(name)) names.push(name);
  };
  for (const match of path.matchAll(
    /\{\{([^{}]*)\}\}|(?<=\/):([A-Za-z_][A-Za-z0-9_-]*)/g,
  )) {
    if (match[2] !== undefined) {
      add(match[2]);
      continue;
    }
    const name = match[1]!.trim();
    if (!name.startsWith("vault:") && !name.startsWith("$")) add(name);
  }
  return names;
}

/** One entry per name, in order. Existing values and descriptions are kept. */
export function syncPathVariables(
  old: Variable[],
  names: string[],
): Variable[] {
  return names.map(
    (name) => old.find((v) => v.name === name) ?? { name, value: "" },
  );
}
