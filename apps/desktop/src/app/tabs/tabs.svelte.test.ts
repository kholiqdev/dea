import { flushSync } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";
import type { UiState } from "$lib/ipc/bindings/UiState";
import { Shell } from "../shell/shell-state.svelte";
import { fakeIpc, railModule, registryWith, view } from "../shell/test-support";
import { Tabs, type CloseChoice } from "./tabs.svelte";

beforeEach(() => {
  fakeIpc();
});

function setup(choice: CloseChoice = "discard") {
  const registry = registryWith(
    railModule("requests", 1, {
      tabTypes: [
        {
          id: "requests.request",
          title: "Request",
          icon: "x",
          view: view("request"),
        },
      ],
    }),
    railModule("database", 2, {
      tabTypes: [
        {
          id: "database.query",
          title: "Query",
          icon: "x",
          view: view("query"),
        },
      ],
    }),
  );
  const shell = new Shell(registry);
  const confirm = vi.fn(() => Promise.resolve(choice));
  const tabs = new Tabs(registry, shell, { confirmClose: confirm });
  return { registry, shell, tabs, confirm };
}

function saved(over: Partial<UiState>): UiState {
  return {
    version: 1,
    activeModule: null,
    activeEnvironment: null,
    railExpanded: false,
    sidebars: {},
    tabs: [],
    activeTab: null,
    closedTabs: [],
    ...over,
  };
}

test("opening a tab activates it and its module", async () => {
  const { tabs, shell } = setup();
  const id = tabs.open("database.query", "harbor_staging", { conn: 1 });
  expect(tabs.active?.id).toBe(id);
  expect(tabs.active?.module).toBe("database");
  await vi.waitFor(() => expect(shell.activeModule).toBe("database"));
});

test("an unknown tab type cannot be opened", () => {
  const { tabs } = setup();
  expect(() => tabs.open("gone.thing", "x", null)).toThrow(
    'Unknown tab type "gone.thing"',
  );
});

test("closing a dirty tab asks first", async () => {
  const cancel = setup("cancel");
  const a = cancel.tabs.open("requests.request", "A", null);
  cancel.tabs.setDirty(a, true);
  expect(await cancel.tabs.close(a)).toBe(false);
  expect(cancel.confirm).toHaveBeenCalledOnce();
  expect(cancel.tabs.tabs.map((t) => t.id)).toEqual([a]);

  const discard = setup("discard");
  const b = discard.tabs.open("requests.request", "B", null);
  discard.tabs.setDirty(b, true);
  expect(await discard.tabs.close(b)).toBe(true);
  expect(discard.tabs.tabs).toEqual([]);

  const save = setup("save");
  const saveHandler = vi.fn(() => Promise.resolve());
  save.tabs.setSaveHandler("requests.request", saveHandler);
  const c = save.tabs.open("requests.request", "C", null);
  save.tabs.setDirty(c, true);
  expect(await save.tabs.close(c)).toBe(true);
  expect(saveHandler).toHaveBeenCalledWith(c);
});

test("a failed save keeps the tab open", async () => {
  const { tabs } = setup("save");
  tabs.setSaveHandler("requests.request", () =>
    Promise.reject(new Error("disk full")),
  );
  const id = tabs.open("requests.request", "A", null);
  tabs.setDirty(id, true);
  await expect(tabs.close(id)).rejects.toThrow("disk full");
  expect(tabs.tabs.map((t) => t.id)).toEqual([id]);
});

test("a clean tab closes without asking", async () => {
  const { tabs, confirm } = setup();
  const id = tabs.open("requests.request", "A", null);
  expect(await tabs.close(id)).toBe(true);
  expect(confirm).not.toHaveBeenCalled();
});

test("closing the active tab moves to the nearest tab of the same module", async () => {
  const { tabs } = setup();
  const a = tabs.open("requests.request", "A", null);
  tabs.open("database.query", "Q", null);
  const b = tabs.open("requests.request", "B", null);
  const c = tabs.open("requests.request", "C", null);
  tabs.activate(b);
  await tabs.close(b);
  expect(tabs.active?.id).toBe(c);
  await tabs.close(c);
  expect(tabs.active?.id).toBe(a);
  await tabs.close(a);
  expect(tabs.active).toBeUndefined();
});

test("reopenLast brings back the last closed tab", async () => {
  const { tabs } = setup();
  const a = tabs.open("requests.request", "A", { n: 1 });
  tabs.open("database.query", "Q", null);
  await tabs.close(a);
  tabs.reopenLast();
  expect(tabs.tabs.map((t) => t.title)).toEqual(["Q", "A"]);
  expect(tabs.active?.title).toBe("A");
  expect(tabs.active?.params).toEqual({ n: 1 });
  tabs.reopenLast();
  expect(tabs.tabs).toHaveLength(2);
});

test("tabs can be reordered", () => {
  const { tabs } = setup();
  const a = tabs.open("requests.request", "A", null);
  const b = tabs.open("requests.request", "B", null);
  const c = tabs.open("database.query", "C", null);
  tabs.move(c, 0);
  expect(tabs.tabs.map((t) => t.id)).toEqual([c, a, b]);
  tabs.move(c, 99);
  expect(tabs.tabs.map((t) => t.id)).toEqual([a, b, c]);
});

test("open tabs are saved and restored", async () => {
  const first = setup();
  const a = first.tabs.open("requests.request", "A", { url: "/a" });
  first.tabs.open("database.query", "Q", null);
  first.tabs.activate(a);
  const snapshot = first.tabs.snapshot();
  expect(snapshot.activeTab).toBe(a);

  const second = setup();
  second.tabs.restore(saved(snapshot));
  await second.shell.openModule("requests");
  expect(second.tabs.tabs.map((t) => [t.title, t.module, t.params])).toEqual([
    ["A", "requests", { url: "/a" }],
    ["Q", "database", null],
  ]);
  expect(second.tabs.active?.id).toBe(a);
});

test("restored_tabs_of_unknown_types_are_dropped", async () => {
  const { tabs, shell } = setup();
  const tab = (id: string, tabType: string) => ({
    id,
    tabType,
    title: id,
    params: null,
  });
  tabs.restore(
    saved({
      tabs: [
        tab("1", "requests.request"),
        tab("2", "gone.thing"),
        tab("3", "database.query"),
      ],
      activeTab: "2",
      closedTabs: [tab("4", "gone.thing"), tab("5", "requests.request")],
    }),
  );
  expect(tabs.tabs.map((t) => t.id)).toEqual(["1", "3"]);
  await shell.openModule("requests");
  expect(tabs.active?.id).toBe("1");
  tabs.reopenLast();
  expect(tabs.active?.id).toBe("5");
  tabs.reopenLast();
  expect(tabs.tabs.map((t) => t.id)).toEqual(["1", "3", "5"]);
});

test("rail navigation shows the module's last tab", async () => {
  const { tabs, shell } = setup();
  const q = tabs.open("database.query", "Q", null);
  const a = tabs.open("requests.request", "A", null);
  // No effect keeps the two in step: the active tab is derived from the
  // shell's module.
  await shell.openModule("database");
  flushSync();
  expect(tabs.active?.id).toBe(q);
  await shell.openModule("requests");
  flushSync();
  expect(tabs.active?.id).toBe(a);
});

test("a tab opened without params is saved with null params", () => {
  const { tabs } = setup();
  tabs.open("requests.request", "A", undefined);
  expect(tabs.snapshot().tabs[0]?.params).toBeNull();
});

test("marking a tab dirty again changes nothing", () => {
  const { tabs } = setup();
  const id = tabs.open("requests.request", "A", null);
  tabs.setDirty(id, true);
  const before = tabs.tabs;
  tabs.setDirty(id, true);
  // Views that report their state on every change rely on this to settle.
  expect(tabs.tabs).toBe(before);
});
