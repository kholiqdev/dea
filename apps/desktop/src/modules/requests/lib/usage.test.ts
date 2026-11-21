import { expect, test } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import { whereUsed } from "./usage";

const base: RequestFile = { name: "r", method: "GET", url: "https://x.test" };
const request = (patch: Partial<RequestFile>): RequestFile => ({
  ...base,
  ...patch,
});

test("where_used_finds_the_first_place", () => {
  const all = request({
    url: "https://x.test/{{id}}",
    params: [
      { name: "a", value: "1" },
      { name: "b", value: "{{id}}" },
    ],
    headers: [{ name: "X-Id", value: "{{id}}" }],
    body: { mode: "json", text: '{"id": "{{id}}"}' },
  });
  expect(whereUsed(all, "id")).toEqual({ section: "url" });
  expect(whereUsed({ ...all, url: "https://x.test" }, "id")).toEqual({
    section: "params",
    row: 1,
  });
  expect(
    whereUsed({ ...all, url: "https://x.test", params: [] }, "id"),
  ).toEqual({ section: "headers", row: 0 });
  expect(
    whereUsed({ ...all, url: "https://x.test", params: [], headers: [] }, "id"),
  ).toEqual({ section: "body" });
  expect(whereUsed(base, "id")).toBeNull();
});

test("where_used_skips_what_is_not_sent", () => {
  const r = request({
    params: [{ name: "a", value: "{{id}}", enabled: false }],
    headers: [
      { name: "b", value: "1" },
      { name: "X-Id", value: "{{id}}" },
    ],
  });
  expect(whereUsed(r, "id")).toEqual({ section: "headers", row: 1 });
});

test("where_used_reads_only_the_url_before_the_query", () => {
  expect(
    whereUsed(request({ url: "https://x.test/?q={{id}}" }), "id"),
  ).toBeNull();
});

test("where_used_matches_names_as_the_inspection_spells_them", () => {
  const r = request({ headers: [{ name: "A", value: "{{ vault: token }}" }] });
  expect(whereUsed(r, "vault:token")).toEqual({ section: "headers", row: 0 });
  expect(whereUsed(r, "token")).toBeNull();
  expect(whereUsed(request({ url: "{{  id  }}" }), "id")).toEqual({
    section: "url",
  });
});

test("where_used_looks_in_the_body_by_mode", () => {
  const form = request({
    body: {
      mode: "urlencoded",
      fields: [
        { name: "a", value: "{{id}}", enabled: false },
        { name: "b", value: "{{id}}" },
      ],
    },
  });
  expect(whereUsed(form, "id")).toEqual({ section: "body" });
  const off = request({
    body: {
      mode: "urlencoded",
      fields: [{ name: "a", value: "{{id}}", enabled: false }],
    },
  });
  expect(whereUsed(off, "id")).toBeNull();
  expect(
    whereUsed(
      request({ body: { mode: "raw", language: "text", text: "{{id}}" } }),
      "id",
    ),
  ).toEqual({ section: "body" });
  expect(
    whereUsed(request({ body: { mode: "binary", file: "{{id}}" } }), "id"),
  ).toBeNull();
});
