import { expect, test } from "vitest";
import { errorMessage } from "./errors";

test("messages from Dea start with a capital letter", () => {
  expect(
    errorMessage({
      kind: "nameTaken",
      message: 'an environment called "qa" already exists',
    }),
  ).toBe('An environment called "qa" already exists');
  expect(errorMessage(new Error("disk full"))).toBe("Disk full");
  expect(errorMessage("")).toBe("");
});
