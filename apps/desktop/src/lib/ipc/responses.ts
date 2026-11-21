// The last response of each tab of this window. The body stays in Rust;
// each wrapper asks for the part the viewer shows. One wrapper per command.

import { invoke } from "@tauri-apps/api/core";
import type { BodyText } from "./bindings/BodyText";
import type { Filtered } from "./bindings/Filtered";
import type { Found } from "./bindings/Found";
import type { RowsPage } from "./bindings/RowsPage";
import type { TextView } from "./bindings/TextView";

export function responseText(id: string, view: TextView): Promise<BodyText> {
  return invoke<BodyText>("response_text", { id, view });
}

/** The body as received, byte for byte. */
export function responseBytes(id: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("response_bytes", { id });
}

export function responseRows(
  id: string,
  from: number,
  count: number,
): Promise<RowsPage> {
  return invoke<RowsPage>("response_rows", { id, from, count });
}

/** The next match of `query` from row `fromRow`, or `null`. */
export function findInResponse(
  id: string,
  query: string,
  fromRow: number,
  backwards: boolean,
): Promise<Found | null> {
  return invoke<Found | null>("find_in_response", {
    id,
    query,
    fromRow,
    backwards,
  });
}

/** The JSON body narrowed to what the JSONPath `path` selects. */
export function filterResponse(id: string, path: string): Promise<Filtered> {
  return invoke<Filtered>("filter_response", { id, path });
}

/** Puts the raw text of the body on the clipboard. */
export function copyResponse(id: string): Promise<void> {
  return invoke<void>("copy_response", { id });
}

/** Asks where to save the body. The name of the saved file, or `null` when cancelled. */
export function saveResponse(id: string, name: string): Promise<string | null> {
  return invoke<string | null>("save_response", { id, name });
}

export function releaseResponse(id: string): Promise<void> {
  return invoke<void>("release_response", { id });
}
