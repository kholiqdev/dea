import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";
import { closeQuestion } from "../prompts/confirm-close.svelte";
import ShellView from "../shell/Shell.svelte";
import { Shell } from "../shell/shell-state.svelte";
import { shellModule } from "../shell/shell-module";
import {
  fakeIpc,
  openProject,
  railModule,
  registryWith,
  view,
} from "../shell/test-support";
import { settings } from "$lib/settings/settings.svelte";
import { Tabs } from "./tabs.svelte";

beforeEach(() => {
  fakeIpc();
});

function setup(...extra: Parameters<typeof registryWith>) {
  const registry = registryWith(
    ...extra,
    railModule("requests", 1, {
      rail: { icon: "arrow-left-right", group: "main", order: 1 },
      tabTypes: [
        {
          id: "requests.request",
          title: "Request",
          icon: "x",
          view: view("request body"),
        },
      ],
    }),
    railModule("database", 2, {
      rail: { icon: "database", group: "main", order: 2 },
      tabTypes: [
        {
          id: "database.query",
          title: "Query",
          icon: "x",
          view: view("query body"),
        },
      ],
    }),
  );
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  const project = openProject();
  registry.register(shellModule(shell, tabs, project));
  render(ShellView, { registry, shell, tabs, project });
  return { registry, shell, tabs };
}

function bar() {
  return screen.getByRole("tablist", { name: "Open tabs" });
}

test("tabs from different modules share one bar with their module icon", async () => {
  const { tabs } = setup();
  tabs.open("requests.request", "Shipment by ID", null);
  tabs.open("database.query", "harbor_staging", null);
  flushSync();
  const all = within(bar()).getAllByRole("tab");
  expect(all.map((t) => t.textContent?.trim())).toEqual([
    "Shipment by ID",
    "harbor_staging",
  ]);
  expect(all[0]?.querySelector(".lucide-arrow-left-right")).not.toBeNull();
  expect(all[1]?.querySelector(".lucide-database")).not.toBeNull();
  expect(await screen.findByText("query body")).toBeInTheDocument();
  await userEvent.click(all[0]!);
  expect(await screen.findByText("request body")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Requests" })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("closing a dirty tab asks Save, Discard or Cancel", async () => {
  const { tabs } = setup();
  const save = vi.fn(() => Promise.resolve());
  tabs.setSaveHandler("requests.request", save);
  const a = tabs.open("requests.request", "A", null);
  tabs.setDirty(a, true);
  flushSync();
  expect(
    within(bar()).getByRole("img", { name: "Unsaved changes" }),
  ).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Close A" }));
  const dialog = await screen.findByRole("dialog", {
    name: "Save changes to A?",
  });
  await userEvent.click(within(dialog).getByRole("button", { name: /Cancel/ }));
  expect(tabs.tabs).toHaveLength(1);

  await userEvent.click(screen.getByRole("button", { name: "Close A" }));
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(tabs.tabs).toHaveLength(1);

  await userEvent.click(screen.getByRole("button", { name: "Close A" }));
  await userEvent.click(await screen.findByRole("button", { name: "Save" }));
  await waitFor(() => expect(tabs.tabs).toHaveLength(0));
  expect(save).toHaveBeenCalledWith(a);

  const b = tabs.open("requests.request", "B", null);
  tabs.setDirty(b, true);
  flushSync();
  await userEvent.click(screen.getByRole("button", { name: "Close B" }));
  await userEvent.click(await screen.findByRole("button", { name: "Discard" }));
  await waitFor(() => expect(tabs.tabs).toHaveLength(0));
});

test("a failed save keeps the tab and says why", async () => {
  const { tabs } = setup();
  tabs.setSaveHandler("requests.request", () =>
    Promise.reject(new Error("Disk is full")),
  );
  const a = tabs.open("requests.request", "A", null);
  tabs.setDirty(a, true);
  flushSync();
  await userEvent.click(screen.getByRole("button", { name: "Close A" }));
  await userEvent.click(await screen.findByRole("button", { name: "Save" }));
  expect(await screen.findByText("Disk is full")).toBeInTheDocument();
  expect(tabs.tabs).toHaveLength(1);
});

test("Mod+W closes and Mod+Shift+T reopens the last closed tab", async () => {
  const { tabs } = setup();
  tabs.open("requests.request", "A", null);
  tabs.open("requests.request", "B", null);
  flushSync();
  await userEvent.keyboard("{Control>}w{/Control}");
  await waitFor(() => expect(tabs.tabs.map((t) => t.title)).toEqual(["A"]));
  await userEvent.keyboard("{Control>}{Shift>}T{/Shift}{/Control}");
  await waitFor(() =>
    expect(tabs.tabs.map((t) => t.title)).toEqual(["A", "B"]),
  );
  expect(tabs.active?.title).toBe("B");
});

test("the keyboard moves between tabs and reorders them", async () => {
  const { tabs } = setup();
  tabs.open("requests.request", "A", null);
  tabs.open("requests.request", "B", null);
  tabs.open("requests.request", "C", null);
  tabs.activate(tabs.tabs[0]!.id);
  flushSync();
  within(bar()).getByRole("tab", { name: "A" }).focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(tabs.active?.title).toBe("B");
  expect(within(bar()).getByRole("tab", { name: "B" })).toHaveFocus();
  await userEvent.keyboard("{Control>}{Shift>}{PageDown}{/Shift}{/Control}");
  await waitFor(() =>
    expect(tabs.tabs.map((t) => t.title)).toEqual(["A", "C", "B"]),
  );
  await userEvent.keyboard("{Control>}{Shift>}{PageUp}{/Shift}{/Control}");
  await waitFor(() =>
    expect(tabs.tabs.map((t) => t.title)).toEqual(["A", "B", "C"]),
  );
});

test("tabs can be dragged to a new place", () => {
  const { tabs } = setup();
  tabs.open("requests.request", "A", null);
  tabs.open("requests.request", "B", null);
  tabs.open("requests.request", "C", null);
  flushSync();
  const els = [...bar().querySelectorAll<HTMLElement>("[data-tab]")];
  els.forEach((el, i) => {
    el.getBoundingClientRect = () => ({ left: i * 100, width: 100 }) as DOMRect;
  });
  const c = els[2]!;
  const pointer = { pointerId: 1, button: 0 };
  c.dispatchEvent(
    new PointerEvent("pointerdown", {
      ...pointer,
      clientX: 250,
      bubbles: true,
    }),
  );
  c.dispatchEvent(
    new PointerEvent("pointermove", {
      ...pointer,
      clientX: 120,
      bubbles: true,
    }),
  );
  c.dispatchEvent(
    new PointerEvent("pointerup", { ...pointer, clientX: 20, bubbles: true }),
  );
  expect(tabs.tabs.map((t) => t.title)).toEqual(["C", "A", "B"]);
});

test("switching between 30 tabs takes under 50 ms each", () => {
  const { tabs } = setup();
  const ids = Array.from({ length: 30 }, (_, i) =>
    tabs.open(i % 2 ? "requests.request" : "database.query", `Tab ${i}`, null),
  );
  flushSync();
  let slowest = 0;
  for (const id of ids) {
    const start = performance.now();
    tabs.activate(id);
    flushSync();
    slowest = Math.max(slowest, performance.now() - start);
  }
  expect(slowest).toBeLessThan(50);
});

test("a production tab shows the production marker", () => {
  const { tabs } = setup();
  tabs.open("database.query", "harbor_prod", null, {
    environment: { name: "production", production: true },
  });
  tabs.open("database.query", "harbor_staging", null, {
    environment: { name: "staging", production: false },
  });
  flushSync();
  const prod = within(bar()).getByRole("tab", { name: /harbor_prod/ });
  const staging = within(bar()).getByRole("tab", { name: /harbor_staging/ });
  expect(prod.querySelector(".env-prod")).toHaveTextContent("production");
  expect(staging.querySelector(".env-prod")).toBeNull();
  expect(staging.querySelector(".tab-env")).toHaveTextContent("staging");
});

test("a tab of a module with no main view shows its content", async () => {
  const { tabs, shell } = setup({
    id: "tool",
    title: "Tool",
    tabTypes: [
      { id: "tool.page", title: "Page", icon: "x", view: view("tool page") },
    ],
  });
  tabs.open("tool.page", "T", null);
  expect(await screen.findByText("tool page")).toBeInTheDocument();
  expect(shell.activeModule).toBe("tool");
});

test("pressing a tab leaves the click to it until a drag starts", () => {
  const { tabs } = setup();
  tabs.open("requests.request", "A", null);
  tabs.open("requests.request", "B", null);
  flushSync();
  const el = bar().querySelectorAll<HTMLElement>("[data-tab]")[1]!;
  const capture = vi.fn();
  el.setPointerCapture = capture;
  const pointer = { pointerId: 7, button: 0, bubbles: true };
  el.dispatchEvent(
    new PointerEvent("pointerdown", { ...pointer, clientX: 150 }),
  );
  // Chromium (WebView2) sends the click to the element holding the
  // capture, so capturing on press would keep the tab from activating.
  expect(capture).not.toHaveBeenCalled();
  el.dispatchEvent(
    new PointerEvent("pointermove", { ...pointer, clientX: 152 }),
  );
  expect(capture).not.toHaveBeenCalled();
  el.dispatchEvent(
    new PointerEvent("pointermove", { ...pointer, clientX: 170 }),
  );
  expect(capture).toHaveBeenCalledWith(7);
});

test("a shortcut the app cannot read does not break the title bar or tab bar", async () => {
  const { tabs, shell } = setup(
    railModule("notes", 3, { sidebar: view("notes sidebar") }),
  );
  await settings.update({
    shortcuts: {
      "shell.toggleSidebar": "Mod+Backquote",
      "shell.closeTab": "Mod+Nope",
    },
  });
  try {
    await shell.openModule("notes");
    tabs.open("requests.request", "A", null);
    flushSync();
    expect(screen.getByRole("button", { name: "Close A" })).toHaveAttribute(
      "title",
      "Close A",
    );
    await shell.openModule("notes");
    flushSync();
    expect(
      screen.getByRole("button", { name: "Hide sidebar" }),
    ).toHaveAttribute("title", "Hide sidebar");
  } finally {
    await settings.update({ shortcuts: {} });
  }
});

test("the active tab is scrolled into view", () => {
  const scrolled: Element[] = [];
  const original = Object.getOwnPropertyDescriptor(
    Element.prototype,
    "scrollIntoView",
  );
  Object.defineProperty(Element.prototype, "scrollIntoView", {
    configurable: true,
    value(this: Element) {
      scrolled.push(this);
    },
  });
  try {
    const { tabs } = setup();
    const a = tabs.open("requests.request", "A", null);
    tabs.open("requests.request", "B", null);
    flushSync();
    scrolled.length = 0;
    tabs.activate(a);
    flushSync();
    expect(scrolled.some((el) => el.closest(`[data-tab="${a}"]`))).toBe(true);
  } finally {
    if (original)
      Object.defineProperty(Element.prototype, "scrollIntoView", original);
    else delete (Element.prototype as Partial<Element>).scrollIntoView;
  }
});

test("closing a tab moves focus to the next tab, then to the work area", async () => {
  const { tabs } = setup();
  tabs.open("requests.request", "A", null);
  const b = tabs.open("requests.request", "B", null);
  tabs.open("requests.request", "C", null);
  tabs.activate(b);
  flushSync();

  await userEvent.click(screen.getByRole("button", { name: "Close B" }));
  await waitFor(() =>
    expect(tabs.tabs.map((t) => t.title)).toEqual(["A", "C"]),
  );
  const active = within(bar()).getByRole("tab", { selected: true });
  expect(active).toHaveTextContent(tabs.active?.title ?? "");
  expect(active).toHaveFocus();

  await userEvent.keyboard("{Control>}w{/Control}");
  await userEvent.keyboard("{Control>}w{/Control}");
  await waitFor(() => expect(tabs.tabs).toHaveLength(0));
  expect(screen.getByRole("main")).toHaveFocus();
});
