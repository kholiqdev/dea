const STANDARD = new Set([
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
]);
const MAX = 7;

/** The `--method-*` token of a standard method; any other is neutral. */
export function methodColor(method: string): string {
  return STANDARD.has(method.toUpperCase())
    ? `var(--method-${method.toLowerCase()})`
    : "var(--text-2)";
}

/** The method cut to seven characters, with an ellipsis when it was longer. */
export function methodLabel(method: string): string {
  return method.length > MAX ? `${method.slice(0, MAX)}…` : method;
}
