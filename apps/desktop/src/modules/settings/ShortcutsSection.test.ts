import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import ShortcutsSection from "./components/ShortcutsSection.svelte";
import {
  fakeSettingsIpc,
  registerTestModules,
  resetSettings,
} from "./test-support";

beforeEach(async () => {
  fakeSettingsIpc();
  registerTestModules();
  await resetSettings();
});

function row(title: string): HTMLElement {
  return screen.getByRole("row", { name: new RegExp(`^${title}`) });
}

test("every registered command is listed with its shortcut", () => {
  render(ShortcutsSection);
  expect(within(row("Open Requests")).getByText("Ctrl")).toBeInTheDocument();
  expect(row("Open Requests")).toHaveTextContent("Ctrl1");
  expect(row("Open Database")).toHaveTextContent("Ctrl2");
});

test("a conflicting shortcut is shown before saving and blocks the save", async () => {
  render(ShortcutsSection);
  await userEvent.click(
    within(row("Open Requests")).getByRole("button", { name: "Change" }),
  );
  const capture = screen.getByRole("textbox", {
    name: "New shortcut for Open Requests",
  });
  expect(capture).toHaveFocus();
  await userEvent.keyboard("{Control>}2{/Control}");
  expect(screen.getByText("Already used by Open Database")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  expect(settings.shortcuts).toEqual({});

  await userEvent.click(
    screen.getByRole("button", { name: "Use it here instead" }),
  );
  expect(settings.shortcuts).toEqual({
    "requests.open": "Mod+2",
    "database.open": null,
  });
  expect(row("Open Database")).toHaveTextContent("Off");
});

test("a free shortcut can be saved", async () => {
  render(ShortcutsSection);
  await userEvent.click(
    within(row("Open Requests")).getByRole("button", { name: "Change" }),
  );
  await userEvent.keyboard("{Control>}{Shift>}r{/Shift}{/Control}");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(settings.shortcuts).toEqual({ "requests.open": "Mod+Shift+R" });
  expect(row("Open Requests")).toHaveTextContent("CtrlShiftR");
});

test("Escape cancels without saving", async () => {
  render(ShortcutsSection);
  await userEvent.click(
    within(row("Open Requests")).getByRole("button", { name: "Change" }),
  );
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(settings.shortcuts).toEqual({});
});

test("a shortcut can be turned off", async () => {
  render(ShortcutsSection);
  await userEvent.click(
    within(row("Open Requests")).getByRole("button", { name: "Change" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Turn off" }));
  expect(settings.shortcuts).toEqual({ "requests.open": null });
  expect(row("Open Requests")).toHaveTextContent("Off");
});

test("reset to default restores the manifest chord", async () => {
  await settings.update({ shortcuts: { "requests.open": "Mod+Shift+1" } });
  render(ShortcutsSection);
  expect(row("Open Requests")).toHaveTextContent("CtrlShift1");
  await userEvent.click(
    within(row("Open Requests")).getByRole("button", {
      name: "Reset to default",
    }),
  );
  expect(settings.shortcuts).toEqual({});
  expect(row("Open Requests")).toHaveTextContent("Ctrl1");
  expect(
    within(row("Open Requests")).queryByRole("button", {
      name: "Reset to default",
    }),
  ).toBeNull();
});

test("the list can be searched", async () => {
  render(ShortcutsSection);
  await userEvent.type(
    screen.getByRole("searchbox", { name: "Search shortcuts" }),
    "data",
  );
  expect(screen.queryByRole("row", { name: /^Open Requests/ })).toBeNull();
  expect(row("Open Database")).toBeInTheDocument();
});

test("a shortcut needs Mod or Alt, except function keys", async () => {
  render(ShortcutsSection);
  await userEvent.click(
    within(row("Open Requests")).getByRole("button", { name: "Change" }),
  );
  const capture = screen.getByRole("textbox", {
    name: "New shortcut for Open Requests",
  });
  // Enter, Space and plain letters would take over typing and buttons.
  await userEvent.keyboard("{Enter}{ }k{Shift>}K{/Shift}");
  expect(capture).toHaveValue("");
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  await userEvent.keyboard("{F5}");
  expect(capture).toHaveValue("F5");
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

test("resetting every shortcut starts on Cancel", async () => {
  await settings.update({ shortcuts: { "requests.open": "Mod+Shift+1" } });
  render(ShortcutsSection);
  await userEvent.click(
    screen.getByRole("button", { name: "Reset all to defaults…" }),
  );
  expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(settings.shortcuts).toEqual({ "requests.open": "Mod+Shift+1" });
});
