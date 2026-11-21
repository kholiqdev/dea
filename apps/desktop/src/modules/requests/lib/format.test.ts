import { expect, test } from "vitest";
import {
  formatBytes,
  formatMs,
  formatSeconds,
  statusTone,
  suggestedName,
} from "./format";

test("format_bytes_and_ms", () => {
  expect(formatBytes(612)).toBe("612 B");
  expect(formatBytes(1536)).toBe("1.5 KB");
  expect(formatBytes(14.2 * 1024 ** 2)).toBe("14.2 MB");
  expect(formatBytes(12 * 1024 ** 2)).toBe("12.0 MB");
  expect(formatMs(148)).toBe("148 ms");
  expect(formatMs(0.4)).toBe("<1 ms");
  expect(formatMs(0)).toBe("0 ms");
  expect(formatMs(1234)).toBe("1.2 s");
  expect(formatSeconds(1234)).toBe("1.2 s");
  expect(formatSeconds(40)).toBe("0.0 s");
});

test("status_tone_follows_the_class", () => {
  expect(statusTone(101)).toBe("muted");
  expect(statusTone(200)).toBe("positive");
  expect(statusTone(302)).toBe("accent");
  expect(statusTone(404)).toBe("warning");
  expect(statusTone(503)).toBe("negative");
});

test("suggested_names_follow_the_request_and_the_body", () => {
  expect(suggestedName("List shipments", "json", "application/json")).toBe(
    "list-shipments.json",
  );
  expect(suggestedName("Export", "binary", null)).toBe("export.bin");
  expect(suggestedName("Logo", "image", "image/svg+xml")).toBe("logo.svg");
  expect(suggestedName("Logo", "image", "image/jpeg")).toBe("logo.jpg");
  expect(suggestedName("Page", "html", "text/html")).toBe("page.html");
  expect(suggestedName("Notes", "text", "text/plain")).toBe("notes.txt");
  expect(suggestedName("  !!  ", "pdf", "application/pdf")).toBe(
    "response.pdf",
  );
});
