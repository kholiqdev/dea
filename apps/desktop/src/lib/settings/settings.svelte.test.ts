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

/** Listens through a fake that the test can send settings with. */
async function listening(settings: ReturnType<typeof createSettings>) {
  let send: (s: Settings) => void = () => {};
  await settings.listen((handler) => {
    send = handler;
    return Promise.resolve(() => {});
  });
  return (s: Settings) => send(s);
}

function withSecurity(autoLockMinutes: number | null): Settings {
  return {
    ...structuredClone(DEFAULT_SETTINGS),
    security: { autoLockMinutes, productionMinutes: 10 },
  };
}

test("settings saved in another window reach this one", async () => {
  const settings = createSettings();
  await settings.load();
  const send = await listening(settings);
  send(withSecurity(30));
  expect(settings.security.autoLockMinutes).toBe(30);
});

test("a change never sends back another window's older values", async () => {
  const settings = createSettings();
  await settings.load();
  const send = await listening(settings);
  // Another window turned auto-lock to 30 minutes.
  send(withSecurity(30));
  await settings.update({ appearance: { theme: "dark" } });
  expect(saved).toHaveLength(1);
  expect(saved[0]?.security.autoLockMinutes).toBe(30);
  expect(saved[0]?.appearance.theme).toBe("dark");
});

test("settings that arrive while a save runs are kept", async () => {
  const settings = createSettings();
  await settings.load();
  const send = await listening(settings);
  let finish: () => void = () => {};
  mockIPC((cmd, args) => {
    if (cmd !== "save_settings") throw new Error(`unexpected ${cmd}`);
    saved.push((args as { settings: Settings }).settings);
    return new Promise<null>((resolve) => {
      finish = () => resolve(null);
    });
  });
  const saving = settings.update({ appearance: { theme: "dark" } });
  await waitForSave();
  // Another window saves auto-lock while this save is on its way.
  send(withSecurity(60));
  finish();
  await saving;
  expect(settings.security.autoLockMinutes).toBe(60);
  expect(settings.appearance.theme).toBe("dark");

  const next = settings.update({ appearance: { fontSize: 14 } });
  await waitForSave(2);
  finish();
  await next;
  expect(saved[1]?.security.autoLockMinutes).toBe(60);
  expect(saved[1]?.appearance).toMatchObject({ theme: "dark", fontSize: 14 });
});

/** Waits until `count` saves were sent. */
async function waitForSave(count = 1): Promise<void> {
  for (let i = 0; i < 50 && saved.length < count; i++)
    await new Promise((r) => setTimeout(r, 0));
  expect(saved).toHaveLength(count);
}

test("the response panel sits to the right by default", () => {
  expect(DEFAULT_SETTINGS.requests.responseLayout).toBe("right");
});
