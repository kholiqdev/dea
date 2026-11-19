import { expect, test } from "vitest";
import { findConflicts } from "./conflicts";

test("finds commands sharing a chord, ignoring disabled ones", () => {
  expect(
    findConflicts({
      "a.x": "Mod+K",
      "b.y": "mod+k",
      "c.z": null,
      "d.w": "Mod+J",
    }),
  ).toEqual([{ chord: "Mod+K", commands: ["a.x", "b.y"] }]);
});

test("no conflicts when every chord is different", () => {
  expect(findConflicts({ "a.x": "Mod+1", "a.y": "Mod+Shift+1" })).toEqual([]);
});
