import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, expect, test } from "vitest";
import type { Settings } from "$lib/ipc/bindings/Settings";
import { DEFAULT_SETTINGS, createSettings } from "./settings.svelte";

let saved: Settings[];
let stored: Settings;
let refuse: string | null;

beforeEach(() => {
  saved = [];
  stored = structuredClone(DEFAULT_SETTINGS);
  refuse = null;
  mockIPC((cmd, args) => {
    if (cmd === "get_settings") return stored;
    if (cmd === "save_settings") {
      // Tauri rejects with the command's error object, not an Error.
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      if (refuse) throw { kind: "invalid", message: refuse };
      stored = (args as { settings: Settings }).settings;
      saved.push(stored);
      return null;
    }
    throw new Error(`unexpected ${cmd}`);
  });
});

afterEach(() => clearMocks());

test("load reads the saved settings", async () => {
  stored.appearance.theme = "light";
  const settings = createSettings();
  expect(settings.appearance.theme).toBe("system");
  await settings.load();
  expect(settings.appearance.theme).toBe("light");
});

test("update saves through IPC and then changes the appearance", async () => {
  const settings = createSettings();
  await settings.load();
  await settings.update({ appearance: { theme: "dark", fontSize: 15 } });
  expect(saved).toHaveLength(1);
  expect(saved[0]?.appearance).toEqual({
    ...DEFAULT_SETTINGS.appearance,
    theme: "dark",
    fontSize: 15,
  });
  expect(settings.appearance.theme).toBe("dark");
  expect(settings.appearance.fontSize).toBe(15);
});

test("quick updates in a row are all kept", async () => {
  const settings = createSettings();
  await settings.load();
  await Promise.all([
    settings.update({ appearance: { fontSize: 14 } }),
    settings.update({ appearance: { theme: "light" } }),
    settings.update({ shortcuts: { "shell.reopenTab": null } }),
  ]);
  expect(stored.appearance.fontSize).toBe(14);
  expect(stored.appearance.theme).toBe("light");
  expect(stored.shortcuts).toEqual({ "shell.reopenTab": null });
  expect(settings.value).toEqual(stored);
});

test("a refused save keeps the old value and reports why", async () => {
  const settings = createSettings();
  await settings.load();
  refuse = "Font size must be 11 to 20";
  await expect(
    settings.update({ appearance: { fontSize: 30 } }),
  ).rejects.toMatchObject({ message: "Font size must be 11 to 20" });
  expect(settings.appearance.fontSize).toBe(13);
  refuse = null;
  await settings.update({ appearance: { fontSize: 12 } });
  expect(settings.appearance.fontSize).toBe(12);
});
