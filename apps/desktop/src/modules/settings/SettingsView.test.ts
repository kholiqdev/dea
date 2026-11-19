import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { registry } from "$app/contributions/registry.svelte";
import SettingsSidebar from "./SettingsSidebar.svelte";
import SettingsView from "./SettingsView.svelte";
import { manifest } from "./index";
import { fakeSettingsIpc } from "./test-support";

beforeEach(() => {
  fakeSettingsIpc();
  if (!registry.module("settings")) registry.register(manifest);
});

test("the sidebar lists the sections and the view shows the chosen one", async () => {
  render(SettingsSidebar);
  render(SettingsView);
  const nav = screen.getByRole("navigation", { name: "Settings sections" });
  expect(nav).toHaveTextContent("AppearanceKeyboard shortcuts");
  expect(
    await screen.findByRole("heading", { name: "Appearance", level: 1 }),
  ).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "Keyboard shortcuts" }),
  );
  expect(
    await screen.findByRole("heading", {
      name: "Keyboard shortcuts",
      level: 1,
    }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Keyboard shortcuts" }),
  ).toHaveAttribute("aria-current", "true");
});
