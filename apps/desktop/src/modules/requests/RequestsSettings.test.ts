import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { Settings } from "$lib/ipc/bindings/Settings";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import RequestsSettings from "./components/RequestsSettings.svelte";

let saved: Settings[];

beforeEach(() => {
  saved = [];
  settings.apply(DEFAULT_SETTINGS);
  clearMocks();
  mockIPC((cmd, args) => {
    if (cmd !== "save_settings") throw new Error(`unexpected command ${cmd}`);
    saved.push((args as { settings: Settings }).settings);
    return null;
  });
});

test("the switch turns autosave on and says when it saves", async () => {
  render(RequestsSettings);
  const toggle = screen.getByRole("switch", {
    name: "Save requests automatically",
  });
  expect(toggle).not.toBeChecked();
  expect(
    screen.getByText("Edits are saved a second after you stop typing."),
  ).toBeInTheDocument();
  await userEvent.click(toggle);
  await waitFor(() => expect(saved.at(-1)?.requests.autosave).toBe(true));
  expect(toggle).toBeChecked();
});
