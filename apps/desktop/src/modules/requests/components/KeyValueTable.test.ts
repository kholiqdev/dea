import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test } from "vitest";
import { sessionView } from "../state/session-view.svelte";
import { lookupFrom } from "../test-support";
import KeyValueTable from "./KeyValueTable.svelte";

afterEach(() => {
  sessionView.descriptions = false;
});

function renderTable(
  rows: { name?: string; value?: string; description?: string }[],
  label = "Query params",
) {
  return render(KeyValueTable, {
    rows: rows.map((r) => ({ name: r.name ?? "page", value: "1", ...r })),
    label,
    addLabel: "Add parameter",
    checkboxLabel: () => "Send",
    variables: lookupFrom(null, null),
    onchange: () => {},
  });
}

test("descriptions_are_hidden_until_asked_for", async () => {
  renderTable([{ description: "First page" }]);
  expect(
    screen.queryByRole("columnheader", { name: "Description" }),
  ).toBeNull();
  expect(screen.queryByRole("textbox", { name: /Description of/ })).toBeNull();
  const toggle = screen.getByRole("button", { name: "Show descriptions" });
  expect(toggle).toHaveAttribute("data-has-descriptions");
  await userEvent.click(toggle);
  expect(
    screen.getByRole("columnheader", { name: "Description" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("textbox", { name: "Description of page" }),
  ).toBeInTheDocument();
  const hide = screen.getByRole("button", { name: "Hide descriptions" });
  expect(hide).toHaveAttribute("aria-pressed", "true");
  expect(hide).not.toHaveAttribute("data-has-descriptions");
});

test("the_choice_is_shared_between_tables", async () => {
  renderTable([{ description: "a" }], "First");
  renderTable([{ description: "b" }], "Second");
  await userEvent.click(
    screen.getAllByRole("button", { name: "Show descriptions" })[0]!,
  );
  expect(
    screen.getAllByRole("columnheader", { name: "Description" }),
  ).toHaveLength(2);
});

test("no_dot_without_descriptions", () => {
  renderTable([{}]);
  expect(
    screen.getByRole("button", { name: "Show descriptions" }),
  ).not.toHaveAttribute("data-has-descriptions");
});
