import { expect, test } from "vitest";
import {
  EnvironmentDraft,
  isProductionName,
  isValidVariableName,
  problemsOf,
} from "./state/environments.svelte";

test("production names match Dea's rule", () => {
  for (const name of ["prod", "Production", " PRD "])
    expect(isProductionName(name), name).toBe(true);
  for (const name of ["preprod", "staging", "prod-eu"])
    expect(isProductionName(name), name).toBe(false);
});

test("variable names follow Dea's rule", () => {
  for (const good of ["baseUrl", "_x", "api.v2", "a-b_c"])
    expect(isValidVariableName(good), good).toBe(true);
  for (const bad of ["", "1x", "a b", "x{"])
    expect(isValidVariableName(bad), bad).toBe(false);
});

test("a draft is dirty only when it differs from disk", () => {
  const d = new EnvironmentDraft({ name: "staging", production: false });
  expect(d.dirty).toBe(false);
  d.addVariable();
  expect(d.dirty).toBe(true);
  d.removeVariable(0);
  expect(d.dirty).toBe(false);
  d.draft.color = "blue";
  expect(d.dirty).toBe(true);
  d.reset({ name: "staging", production: false, color: "blue" });
  expect(d.dirty).toBe(false);
});

test("problems are listed before saving", () => {
  expect(
    problemsOf(
      {
        name: " ",
        production: false,
        variables: [
          { name: "1x", value: "" },
          { name: "baseUrl", value: "" },
          { name: "baseUrl", value: "" },
        ],
      },
      [],
    ),
  ).toEqual([
    "An environment needs a name",
    '"1x" is not a variable name: start with a letter or _, then use letters, digits, _, . or -',
    'The variable "baseUrl" is listed twice',
  ]);
  expect(
    problemsOf({ name: "Staging", production: false }, ["staging"]),
  ).toEqual(['An environment called "Staging" already exists']);
});
