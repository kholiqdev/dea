import { expect, test } from "vitest";
import { columnWidths } from "./columns";

test("key_and_value_split_the_free_width_35_65", () => {
  expect(columnWidths(396, 72, false)).toEqual({ key: 113, description: 0 });
});

test("shown_descriptions_split_28_44_28", () => {
  expect(columnWidths(396, 72, true)).toEqual({ key: 91, description: 91 });
});

test("an_unmeasured_table_gets_no_widths", () => {
  expect(columnWidths(0, 72, false)).toBeNull();
});
