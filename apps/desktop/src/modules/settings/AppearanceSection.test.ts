import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import AppearanceSection from "./components/AppearanceSection.svelte";
import { fakeSettingsIpc, resetSettings } from "./test-support";

let ipc: ReturnType<typeof fakeSettingsIpc>;
beforeEach(async () => {
  ipc = fakeSettingsIpc();
  await resetSettings();
  ipc.saved.length = 0;
});

test("choosing Light applies at once and is saved", async () => {
  render(AppearanceSection);
  const theme = screen.getByRole("group", { name: "Theme" });
  expect(screen.getByRole("button", { name: "Match system" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await userEvent.click(screen.getByRole("button", { name: "Light" }));
  expect(settings.appearance.theme).toBe("light");
  expect(ipc.saved.at(-1)?.appearance.theme).toBe("light");
  expect(theme.querySelector('[aria-pressed="true"]')).toHaveTextContent(
    "Light",
  );
});

test("font size is limited to 11 to 20", async () => {
  render(AppearanceSection);
  const size = screen.getByRole("spinbutton", { name: "Size in px" });
  const larger = screen.getByRole("button", { name: "Larger" });
  const smaller = screen.getByRole("button", { name: "Smaller" });
  await userEvent.click(larger);
  expect(settings.appearance.fontSize).toBe(14);

  await userEvent.clear(size);
  await userEvent.type(size, "30{Enter}");
  expect(settings.appearance.fontSize).toBe(20);
  expect(size).toHaveValue(20);
  expect(larger).toBeDisabled();

  await userEvent.clear(size);
  await userEvent.type(size, "3{Enter}");
  expect(settings.appearance.fontSize).toBe(11);
  expect(smaller).toBeDisabled();
  expect(
    ipc.saved.every(
      (s) => s.appearance.fontSize >= 11 && s.appearance.fontSize <= 20,
    ),
  ).toBe(true);
});

test("fonts come from a short list or a typed name", async () => {
  render(AppearanceSection);
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Interface font" }),
    "System UI",
  );
  expect(settings.appearance.uiFont).toBe("System UI");

  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Monospace font" }),
    "other",
  );
  const name = screen.getByRole("textbox", { name: "Monospace font name" });
  await userEvent.type(name, "JetBrains Mono{Enter}");
  expect(settings.appearance.monoFont).toBe("JetBrains Mono");
});

test("the preview shows a tab, an editor and a terminal", () => {
  render(AppearanceSection);
  const preview = screen.getByRole("figure", { name: "Preview" });
  expect(preview).toHaveTextContent("harbor_production");
  expect(preview.querySelector(".editor")).not.toBeNull();
  expect(preview.querySelector(".terminal")).not.toBeNull();
});
