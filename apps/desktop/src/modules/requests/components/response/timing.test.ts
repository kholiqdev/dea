import { expect, test } from "vitest";
import { sizeTier } from "./timing";

test.each([
  [0, "wide"],
  [479, "tiny"],
  [480, "small"],
  [559, "small"],
  [560, "medium"],
  [679, "medium"],
  [680, "wide"],
  [800, "wide"],
])("a_header_%i_px_wide_is_%s", (width, tier) => {
  expect(sizeTier(width)).toBe(tier);
});
