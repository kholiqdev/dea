import { expect, test } from "vitest";
import { isValidSecretName, vaultReferences } from "./references";

test("references name the secrets a value uses", () => {
  expect(
    vaultReferences("Bearer {{vault:access_token}} {{ vault: b }}"),
  ).toEqual(["access_token", "b"]);
  expect(vaultReferences("plain")).toEqual([]);
});

test("a secret name follows the variable name rule", () => {
  for (const good of ["a", "_x", "api-key.v2"])
    expect(isValidSecretName(good)).toBe(true);
  for (const bad of ["", "1a", "a b", "-a"])
    expect(isValidSecretName(bad)).toBe(false);
});
