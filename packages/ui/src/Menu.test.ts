import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { expect, test } from "vitest";
import Menu, { type MenuItem } from "./Menu.svelte";

const trigger = createRawSnippet(() => ({ render: () => "<span>Row</span>" }));

function item(id: string, extra: Partial<MenuItem> = {}): MenuItem {
  return { id, label: id, onselect: () => {}, ...extra };
}

function open(items: MenuItem[]) {
  return render(Menu, { open: true, items, menuLabel: "Row", trigger });
}

test("a_separator_sits_before_its_item", () => {
  open([item("one"), item("two"), item("three", { separatorBefore: true })]);
  const third = screen.getAllByRole("menuitem")[2];
  const separator = screen.getByRole("separator");
  expect(third?.previousElementSibling).toBe(separator);
  expect(screen.getAllByRole("separator")).toHaveLength(1);
});

test("arrow_keys_skip_separators", async () => {
  open([item("one"), item("two"), item("three", { separatorBefore: true })]);
  const items = screen.getAllByRole("menuitem");
  items[1]?.focus();
  await userEvent.keyboard("{ArrowDown}");
  expect(items[2]).toHaveFocus();
  await userEvent.keyboard("{ArrowUp}");
  expect(items[1]).toHaveFocus();
});

test("a_danger_item_is_marked", () => {
  open([item("one"), item("delete", { tone: "danger" })]);
  expect(screen.getByRole("menuitem", { name: "delete" })).toHaveAttribute(
    "data-tone",
    "danger",
  );
  expect(screen.getByRole("menuitem", { name: "one" })).not.toHaveAttribute(
    "data-tone",
  );
});
