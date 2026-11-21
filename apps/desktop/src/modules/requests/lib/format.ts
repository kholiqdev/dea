// How the response pane writes sizes, times and file names.

import type { BodyKind } from "$lib/ipc/bindings/BodyKind";

const UNITS = ["B", "KB", "MB", "GB"];

/** `612 B`, `1.5 KB`, `14.2 MB`; units of 1024. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${Math.round(n)} B`;
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(1)} ${UNITS[unit]}`;
}

/** `148 ms`, `<1 ms`, `1.2 s`. */
export function formatMs(ms: number): string {
  if (ms <= 0) return "0 ms";
  if (ms < 1) return "<1 ms";
  const rounded = Math.round(ms);
  return rounded < 1000 ? `${rounded} ms` : formatSeconds(ms);
}

/** Always in seconds, to a tenth: `0.4 s`, `1.2 s`. */
export function formatSeconds(ms: number): string {
  return `${(ms / 1000).toFixed(1)} s`;
}

export type StatusTone =
  "muted" | "positive" | "accent" | "warning" | "negative";

/** The colour family of a status: 2xx good, 3xx redirect, 4xx client, 5xx server. */
export function statusTone(status: number): StatusTone {
  if (status >= 500) return "negative";
  if (status >= 400) return "warning";
  if (status >= 300) return "accent";
  if (status >= 200) return "positive";
  return "muted";
}

const IMAGE_EXTENSIONS: Record<string, string> = {
  "image/jpeg": "jpg",
  "image/svg+xml": "svg",
  "image/png": "png",
  "image/gif": "gif",
  "image/webp": "webp",
};

function extension(kind: BodyKind, contentType: string | null): string {
  switch (kind) {
    case "json":
    case "xml":
    case "html":
    case "pdf":
      return kind;
    case "image": {
      const type = contentType?.split(";")[0]?.trim().toLowerCase() ?? "";
      return IMAGE_EXTENSIONS[type] ?? "bin";
    }
    case "binary":
      return "bin";
    default:
      return "txt";
  }
}

/** `("List shipments", "json")` is `list-shipments.json`. */
export function suggestedName(
  requestName: string,
  kind: BodyKind,
  contentType: string | null,
): string {
  const slug = requestName
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `${slug || "response"}.${extension(kind, contentType)}`;
}

const COOKIE_DATE = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
});

/** `4 Nov 2026`; `Session` for a cookie with no expiry. */
export function formatCookieDate(expires: string | null): string {
  return expires === null ? "Session" : COOKIE_DATE.format(new Date(expires));
}
