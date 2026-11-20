import { expect, test } from "vitest";
import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
import type { Variable } from "$lib/ipc/bindings/Variable";
import {
  joinUrl,
  pathVariableNames,
  splitUrl,
  syncParams,
  syncPathVariables,
} from "./url";

test("table_values_with_separators_survive_the_round_trip", () => {
  const params = [{ name: "q", value: "a&b=c#d", enabled: true }];
  const url = joinUrl("{{baseUrl}}/search", params);
  expect(url).toBe("{{baseUrl}}/search?q=a%26b=c%23d");
  expect(splitUrl(url).query).toEqual([{ name: "q", value: "a%26b=c%23d" }]);
});

test("path variables come from :name and {{name}} after the host", () => {
  expect(
    pathVariableNames(
      "{{baseUrl}}/v1/shipments/{{shipmentId}}/events/:eventId?x={{y}}",
    ),
  ).toEqual(["shipmentId", "eventId"]);
  expect(pathVariableNames("https://api.test:8443/users/:id")).toEqual(["id"]);
});

test("disabled_params_keep_their_place_when_the_url_changes", () => {
  const old: KeyValue[] = [
    { name: "a", value: "1", description: "first" },
    { name: "off", value: "x", enabled: false },
    { name: "b", value: "2", description: "second" },
    { name: "off2", value: "y", enabled: false },
  ];
  const next = syncParams(old, [
    { name: "a", value: "9" },
    { name: "b", value: "2" },
    { name: "c", value: "3" },
  ]);
  expect(next).toEqual([
    { name: "a", value: "9", description: "first" },
    { name: "off", value: "x", enabled: false },
    { name: "b", value: "2", description: "second" },
    { name: "off2", value: "y", enabled: false },
    { name: "c", value: "3" },
  ]);
  expect(joinUrl("/x", next)).toBe("/x?a=9&b=2&c=3");
  const fewer = syncParams(old, [{ name: "a", value: "1" }]);
  expect(fewer.map((p) => p.name)).toEqual(["a", "off", "off2"]);
});

test("description_is_dropped_when_the_name_at_that_position_changes", () => {
  const old: KeyValue[] = [{ name: "a", value: "1", description: "d" }];
  expect(syncParams(old, [{ name: "z", value: "1" }])).toEqual([
    { name: "z", value: "1" },
  ]);
});

test("question_mark_inside_a_variable_is_not_the_query", () => {
  expect(splitUrl("{{a?b}}/x")).toEqual({ base: "{{a?b}}/x", query: [] });
  expect(splitUrl("{{a?b}}/x?k={{v&w=z}}&m=1")).toEqual({
    base: "{{a?b}}/x",
    query: [
      { name: "k", value: "{{v&w=z}}" },
      { name: "m", value: "1" },
    ],
  });
});

test("empty_value_and_bare_name_round_trip", () => {
  const { base, query } = splitUrl("/x?a=&b");
  expect(base).toBe("/x");
  expect(query).toEqual([
    { name: "a", value: "" },
    { name: "b", value: "" },
  ]);
  expect(joinUrl(base, query)).toBe("/x?a=&b=");
});

test("non_ascii_and_spaces_are_kept_as_typed", () => {
  const { base, query } = splitUrl("/cari?kota=Jakarta Selatan&nama=Zoë é");
  expect(query).toEqual([
    { name: "kota", value: "Jakarta Selatan" },
    { name: "nama", value: "Zoë é" },
  ]);
  expect(joinUrl(base, query)).toBe("/cari?kota=Jakarta Selatan&nama=Zoë é");
});

test("only_enabled_params_are_joined_and_names_escape_equals", () => {
  const params: KeyValue[] = [
    { name: "a=b", value: "1=2" },
    { name: "off", value: "x", enabled: false },
  ];
  expect(joinUrl("/x", params)).toBe("/x?a%3Db=1=2");
  expect(joinUrl("/x", [params[1]!])).toBe("/x");
});

test("vault_and_dynamic_variables_are_not_path_variables", () => {
  expect(
    pathVariableNames("{{baseUrl}}/{{vault:t}}/{{$uuid}}/:id/:id/{{a}}/{{a}}"),
  ).toEqual(["id", "a"]);
  expect(pathVariableNames("{{baseUrl}}")).toEqual([]);
  expect(pathVariableNames("https://h.test:8080")).toEqual([]);
  expect(pathVariableNames("https://{{host}}/u/:id#frag/:no")).toEqual(["id"]);
});

test("path_variable_values_are_kept_for_names_that_remain", () => {
  const old: Variable[] = [
    { name: "id", value: "7", description: "user" },
    { name: "gone", value: "x" },
  ];
  expect(syncPathVariables(old, ["id", "new"])).toEqual([
    { name: "id", value: "7", description: "user" },
    { name: "new", value: "" },
  ]);
});
