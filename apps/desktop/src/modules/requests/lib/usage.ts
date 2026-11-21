// Where a request uses a variable, so the warning can point at it.

import { variableRanges } from "@dea/editor";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import { key } from "../state/variables.svelte";

/** A place in a request, as the editor shows it. */
export type Usage =
  | { section: "url" }
  | { section: "params" | "headers"; row: number }
  | { section: "body" };

function uses(text: string | undefined, name: string): boolean {
  if (!text) return false;
  return variableRanges(text).some((r) => key(r.name) === name);
}

type Row = { name: string; value?: string; enabled?: boolean };

const inRow = (row: Row, name: string) =>
  row.enabled !== false && (uses(row.name, name) || uses(row.value, name));

/**
 * The first place `{{name}}` is used: the URL before its `?`, then the
 * rows of Params, those of Headers, then the body. A row that is switched
 * off is not sent, so it does not count. `name` is spelled as the
 * inspection lists it, `vault:token` for a secret.
 */
export function whereUsed(request: RequestFile, name: string): Usage | null {
  const wanted = key(name);
  if (uses(request.url.split("?")[0], wanted)) return { section: "url" };
  const params = (request.params ?? []).findIndex((r) => inRow(r, wanted));
  if (params >= 0) return { section: "params", row: params };
  const headers = (request.headers ?? []).findIndex((r) => inRow(r, wanted));
  if (headers >= 0) return { section: "headers", row: headers };
  const body = request.body;
  if (!body) return null;
  switch (body.mode) {
    case "json":
    case "raw":
      return uses(body.text, wanted) ? { section: "body" } : null;
    case "urlencoded":
    case "formData":
      return body.fields.some((r) => inRow(r, wanted))
        ? { section: "body" }
        : null;
    case "binary":
      return null;
  }
}
