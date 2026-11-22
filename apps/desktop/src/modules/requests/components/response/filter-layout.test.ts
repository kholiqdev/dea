import { expect, test } from "vitest";
import { filterLayout } from "./filter-layout";

const at = (pane: number) => ({
  pane,
  sections: 210,
  buttons: 92,
  dropdown: 71,
});

test("buttons_while_they_fit_beside_a_resting_field", () => {
  expect(filterLayout(at(530), false)).toEqual({
    views: "buttons",
    keys: true,
  });
});

test("a_dropdown_when_the_buttons_do_not_fit", () => {
  expect(filterLayout(at(520), false)).toEqual({
    views: "dropdown",
    keys: true,
  });
});

test("no_keycaps_when_even_the_dropdown_leaves_too_little", () => {
  expect(filterLayout(at(480), false)).toEqual({
    views: "dropdown",
    keys: false,
  });
});

test("a_filter_in_use_hides_the_views", () => {
  expect(filterLayout(at(1200), true)).toEqual({
    views: "hidden",
    keys: false,
  });
});

test("an_unmeasured_pane_counts_as_wide", () => {
  expect(filterLayout(at(0), false)).toEqual({ views: "buttons", keys: true });
});
