import { beforeEach, expect, test, vi } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import { clampSplit, setResponseSplit } from "./layout";

beforeEach(() => vi.restoreAllMocks());

test("clamp split rounds and clamps", () => {
  expect(clampSplit(44.6)).toBe(45);
  expect(clampSplit(3)).toBe(20);
  expect(clampSplit(97)).toBe(80);
});

test("saving a split keeps the other layout", async () => {
  const update = vi.spyOn(settings, "update").mockResolvedValue();
  await setResponseSplit("right", 52);
  expect(update).toHaveBeenCalledWith({
    requests: { responseSplit: { right: 52, below: 40 } },
  });
});
