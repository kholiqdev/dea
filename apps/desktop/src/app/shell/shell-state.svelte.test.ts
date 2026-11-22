import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { flushSync } from "svelte";
import {
  Shell,
  clampSidebarWidth,
  emptyUiState,
  persistUiState,
} from "./shell-state.svelte";
import {
  fakeIpc,
  railModule,
  registryWith,
  view,
  type IpcLog,
} from "./test-support";

let ipc: IpcLog;
beforeEach(() => {
  ipc = fakeIpc();
});
afterEach(() => vi.useRealTimers());

test("sidebar_width_is_clamped_to_the_window", () => {
  expect(clampSidebarWidth(900, 1024, 52)).toBe(1024 - 52 - 480);
  expect(clampSidebarWidth(100, 1440, 52)).toBe(200);
  expect(clampSidebarWidth(320, 1440, 52)).toBe(320);
  // A window too small for both keeps the sidebar's minimum.
  expect(clampSidebarWidth(320, 600, 216)).toBe(200);
});

test("the_last_requested_module_wins", async () => {
  const terminalView = view("terminal view");
  const r = registryWith(
    railModule("database", 2, { view: view("database view", 200) }),
    railModule("terminal", 3, { view: terminalView }),
  );
  const shell = new Shell(r);
  const slow = shell.openModule("database");
  const fast = shell.openModule("terminal");
  await Promise.all([slow, fast]);
  expect(shell.activeModule).toBe("terminal");
  // The late Database view must not replace Terminal's.
  expect(shell.activeView).toBe((await terminalView()).default);
});

test("a module with only tab types can be active, with no main view", async () => {
  const r = registryWith(railModule("database", 2), {
    id: "tool",
    title: "Tool",
    tabTypes: [
      { id: "tool.page", title: "Page", icon: "x", view: view("page") },
    ],
  });
  const shell = new Shell(r);
  await shell.openModule("database");
  await shell.openModule("tool");
  expect(shell.activeModule).toBe("tool");
  expect(shell.activeView).toBeNull();
});

test("opening an unknown or disabled module changes nothing", async () => {
  const r = registryWith(railModule("database", 2));
  const shell = new Shell(r);
  await shell.openModule("database");
  await shell.openModule("nope");
  r.setEnabled("database", false);
  await shell.openModule("database");
  expect(shell.activeModule).toBe("database");
});

test("sidebars default to visible at the design width and can be toggled", () => {
  const shell = new Shell(registryWith());
  expect(shell.sidebar("requests")).toEqual({ visible: true, width: 280 });
  shell.toggleSidebar("requests");
  shell.setSidebarWidth("requests", 333.4);
  expect(shell.sidebar("requests")).toEqual({ visible: false, width: 333 });
  expect(shell.sidebar("database").visible).toBe(true);
});

test("restore takes the saved state", () => {
  const shell = new Shell(registryWith());
  shell.restore({
    version: 1,
    activeModule: "terminal",
    activeEnvironment: null,
    railExpanded: true,
    sidebars: { requests: { visible: false, width: 300 } },
    tabs: [],
    activeTab: null,
    closedTabs: [],
  });
  expect(shell.railExpanded).toBe(true);
  expect(shell.sidebar("requests")).toEqual({ visible: false, width: 300 });
});

test("sidebar width and visibility are saved after a pause", async () => {
  vi.useFakeTimers();
  const shell = new Shell(registryWith());
  const stop = persistUiState("no-project", 0, () => shell.snapshot());
  flushSync();
  shell.setSidebarWidth("requests", 320);
  flushSync();
  shell.toggleSidebar("requests");
  flushSync();
  expect(ipc.savedUiStates).toEqual([]);
  await vi.advanceTimersByTimeAsync(300);
  expect(ipc.savedUiStates).toHaveLength(1);
  expect(ipc.savedUiStates[0]?.scope).toBe("no-project");
  expect(ipc.savedUiStates[0]?.slot).toBe(0);
  expect(ipc.savedUiStates[0]?.state.sidebars).toEqual({
    requests: { visible: false, width: 320 },
  });
  stop();
});

test("showing_the_sidebar_while_narrow_is_not_saved", () => {
  const shell = new Shell(registryWith(railModule("requests", 1)));
  shell.narrow = true;
  expect(shell.sidebarShown("requests")).toBe(false);
  shell.toggleSidebar("requests");
  expect(shell.sidebarShown("requests")).toBe(true);
  expect(shell.snapshot().sidebars.requests?.visible ?? true).toBe(true);
  shell.narrow = false;
  shell.narrow = true;
  expect(shell.sidebarShown("requests")).toBe(false);
});

test("showing_a_sidebar_for_a_module_keeps_a_narrow_window_unsaved", () => {
  const shell = new Shell(registryWith(railModule("requests", 1)));
  shell.restore({
    ...emptyUiState(),
    sidebars: { requests: { visible: false, width: 300 } },
  });
  shell.narrow = true;
  shell.showSidebar("requests");
  expect(shell.sidebarShown("requests")).toBe(true);
  expect(shell.snapshot().sidebars).toEqual({
    requests: { visible: false, width: 300 },
  });
  // Wide, it is shown for good.
  shell.narrow = false;
  expect(shell.sidebarShown("requests")).toBe(false);
  shell.showSidebar("requests");
  expect(shell.sidebarShown("requests")).toBe(true);
  expect(shell.snapshot().sidebars).toEqual({
    requests: { visible: true, width: 300 },
  });
});

test("a_sidebar_hidden_by_the_user_stays_hidden_when_wide", () => {
  const shell = new Shell(registryWith(railModule("requests", 1)));
  shell.restore({
    ...emptyUiState(),
    sidebars: { requests: { visible: false, width: 280 } },
  });
  shell.narrow = false;
  expect(shell.sidebarShown("requests")).toBe(false);
});
