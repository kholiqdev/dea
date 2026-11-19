import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import DialogHost from "./test-fixtures/DialogHost.svelte";

test("traps focus, closes on Escape and returns focus", async () => {
  render(DialogHost);
  const trigger = screen.getByRole("button", { name: "Open" });
  await userEvent.click(trigger);

  const dialog = screen.getByRole("dialog", { name: "Save changes?" });
  expect(dialog).toHaveAttribute("aria-modal", "true");
  // The marked action has focus first.
  expect(screen.getByRole("button", { name: "Save" })).toHaveFocus();

  // Tab and Shift+Tab stay inside the dialog.
  for (let i = 0; i < 4; i++) {
    await userEvent.tab();
    expect(dialog.contains(document.activeElement)).toBe(true);
  }
  for (let i = 0; i < 4; i++) {
    await userEvent.tab({ shift: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
  }

  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("1");
  expect(trigger).toHaveFocus();
});

test("focus goes back to the trigger when an action closes it", async () => {
  render(DialogHost);
  const trigger = screen.getByRole("button", { name: "Open" });
  await userEvent.click(trigger);
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

test("the rest of the page is inert while the dialog is open", async () => {
  render(DialogHost);
  const outside = screen.getByRole("button", { name: "Outside" });
  await userEvent.click(screen.getByRole("button", { name: "Open" }));
  expect(outside).toHaveAttribute("inert");
  expect(screen.getByRole("dialog")).not.toHaveAttribute("inert");
  expect(screen.getByRole("dialog").closest("[inert]")).toBeNull();

  await userEvent.keyboard("{Escape}");
  expect(outside).not.toHaveAttribute("inert");
  expect(screen.getByRole("button", { name: "Open" })).toHaveFocus();
});

test("a hidden title still names the dialog", async () => {
  const { createRawSnippet } = await import("svelte");
  const { default: Dialog } = await import("./Dialog.svelte");
  render(Dialog, {
    title: "Search",
    hideTitle: true,
    oncancel: () => {},
    children: createRawSnippet(() => ({ render: () => "<p>Body</p>" })),
  });
  expect(screen.getByRole("dialog", { name: "Search" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Search" })).toHaveClass(
    "visually-hidden",
  );
});
