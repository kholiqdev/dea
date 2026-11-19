// The command palette (F1-FON-11): Mod+K, grouped results, the keyboard.

import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import type { PaletteItem } from "../contributions/types";
import { closeQuestion } from "../prompts/confirm-close.svelte";
import { menus } from "../shell/menus.svelte";
import ShellView from "../shell/Shell.svelte";
import { Shell } from "../shell/shell-state.svelte";
import { shellModule } from "../shell/shell-module";
import {
  fakeIpc,
  openProject,
  projectInfo,
  railModule,
  registryWith,
} from "../shell/test-support";
import { Tabs } from "../tabs/tabs.svelte";

let opened: { id: string; target: string }[];
let sectionsOpened: unknown[];
let sourceItems: PaletteItem[];

function item(id: string, extra: Partial<PaletteItem> = {}): PaletteItem {
  return {
    id,
    title: id,
    open: (target) => void opened.push({ id, target }),
    ...extra,
  };
}

beforeEach(() => {
  // The palette's open state belongs to the window, so it outlives a test.
  menus.palette = false;
  fakeIpc();
  opened = [];
  sectionsOpened = [];
  sourceItems = [
    item("staging", {
      meta: "Use for requests",
      environment: { name: "staging", production: false },
    }),
    item("production", {
      meta: "Use for requests",
      environment: { name: "production", production: true },
    }),
  ];
});

function setup(
  opts: { project?: boolean; failing?: boolean; items?: PaletteItem[] } = {},
) {
  const sections = {
    id: "settings",
    title: "Settings",
    scope: "app" as const,
    commands: [
      {
        id: "settings.open",
        title: "Open Settings",
        action: { type: "openModule" as const, module: "settings" },
      },
    ],
    shortcuts: [{ command: "settings.open", chord: "Mod+," }],
    intents: [
      {
        intent: "settings.openSection",
        handler: () =>
          Promise.resolve({
            default: (args: unknown) => void sectionsOpened.push(args),
          }),
      },
    ],
    settings: [
      {
        id: "settings.appearance",
        title: "Appearance",
        order: 1,
        view: railModule("x", 9).view!,
      },
      {
        id: "settings.shortcuts",
        title: "Keyboard shortcuts",
        order: 2,
        view: railModule("y", 9).view!,
      },
    ],
  };
  const registry = registryWith(
    railModule("requests", 1, {
      searchSources: [
        {
          id: "requests.environments",
          group: "Environments",
          order: 60,
          source: () =>
            Promise.resolve({ default: () => opts.items ?? sourceItems }),
        },
        ...(opts.failing
          ? [
              {
                id: "requests.broken",
                group: "Broken",
                order: 10,
                source: () =>
                  Promise.resolve({
                    default: () => {
                      throw new Error("source failed");
                    },
                  }),
              },
            ]
          : []),
      ],
    }),
    sections,
  );
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  const project = openProject(opts.project === false ? null : projectInfo());
  registry.register(shellModule(shell, tabs, project));
  registry.setProjectOpen(opts.project !== false);
  render(ShellView, { registry, shell, tabs, project });
  return { registry, shell };
}

async function openPalette() {
  await userEvent.keyboard("{Control>}k{/Control}");
  return screen.findByRole("dialog", { name: "Search" });
}

function groups(dialog: HTMLElement) {
  return within(dialog)
    .getAllByRole("group")
    .map((g) => [
      g.getAttribute("aria-label"),
      within(g)
        .getAllByRole("option")
        .map((o) => o.querySelector(".label")?.textContent),
    ]);
}

test("Mod+K opens the palette with commands and settings grouped", async () => {
  setup();
  const dialog = await openPalette();
  expect(
    within(dialog).getByRole("combobox", { name: "Search" }),
  ).toHaveAttribute(
    "placeholder",
    "Search requests, tables, hosts, settings and commands",
  );
  const found = groups(dialog);
  expect(found.map(([g]) => g)).toEqual([
    "Environments",
    "Settings",
    "Commands",
  ]);
  expect(found[1]?.[1]).toEqual(["Appearance", "Keyboard shortcuts"]);
  // The palette's own command is not listed.
  expect(within(dialog).queryByText("Open the command palette")).toBeNull();
});

test("typing ranks items and keeps them grouped in group order", async () => {
  setup();
  const dialog = await openPalette();
  await userEvent.type(within(dialog).getByRole("combobox"), "op");
  expect(groups(dialog)).toEqual([
    ["Commands", ["Open requests", "Open Settings", "Reopen closed tab"]],
  ]);
  await userEvent.clear(within(dialog).getByRole("combobox"));
  await userEvent.type(within(dialog).getByRole("combobox"), "s");
  const names = groups(dialog).map(([g]) => g);
  expect(names).toEqual(["Environments", "Settings", "Commands"]);
});

test("arrows, Home and End move the active row; Enter runs it", async () => {
  setup();
  const dialog = await openPalette();
  const box = within(dialog).getByRole("combobox");
  const active = () =>
    document
      .getElementById(box.getAttribute("aria-activedescendant") ?? "")
      ?.querySelector(".label")?.textContent;
  expect(active()).toBe("staging");
  await userEvent.keyboard("{ArrowDown}");
  expect(active()).toBe("production");
  // Without a query each group shows its first five items.
  await userEvent.keyboard("{End}");
  expect(active()).toBe("Clear local data…");
  await userEvent.keyboard("{ArrowDown}");
  expect(active()).toBe("staging");
  await userEvent.keyboard("{ArrowUp}");
  expect(active()).toBe("Clear local data…");
  await userEvent.keyboard("{Home}{Enter}");
  expect(opened).toEqual([{ id: "staging", target: "active" }]);
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("Mod+Enter opens a tab item in a new tab", async () => {
  setup({
    items: [item("Shipment by ID", { opensTab: true }), item("staging")],
  });
  await openPalette();
  await userEvent.keyboard("{Control>}{Enter}{/Control}");
  expect(opened).toEqual([{ id: "Shipment by ID", target: "new" }]);
  const dialog = await openPalette();
  await userEvent.keyboard("{ArrowDown}{Control>}{Enter}{/Control}");
  // Not a tab: Mod+Enter acts like Enter.
  expect(opened.at(-1)).toEqual({ id: "staging", target: "active" });
  expect(dialog).not.toBeInTheDocument();
});

test("Mod+K toggles the palette and Esc gives focus back", async () => {
  setup();
  const rail = screen.getByRole("button", { name: "Requests" });
  rail.focus();
  await openPalette();
  await userEvent.keyboard("{Control>}k{/Control}");
  expect(screen.queryByRole("dialog")).toBeNull();
  await openPalette();
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(rail).toHaveFocus();
});

test("a changed palette shortcut opens it and shows on the Search box", async () => {
  setup();
  await settings.update({ shortcuts: { "shell.palette": "Mod+Shift+P" } });
  flushSync();
  const search = screen.getByRole("button", { name: /^Search/ });
  expect(search).toHaveTextContent(/P/);
  await userEvent.keyboard("{Control>}k{/Control}");
  expect(screen.queryByRole("dialog")).toBeNull();
  await userEvent.keyboard("{Control>}{Shift>}P{/Shift}{/Control}");
  expect(await screen.findByRole("dialog", { name: "Search" })).toBeTruthy();
  await userEvent.keyboard("{Escape}");
  await userEvent.click(search);
  expect(await screen.findByRole("dialog", { name: "Search" })).toBeTruthy();
  await settings.update({ shortcuts: {} });
});

test("production items carry the production badge", async () => {
  setup();
  const dialog = await openPalette();
  const prod = within(dialog).getByRole("option", { name: /^production/ });
  expect(prod.querySelector(".env-prod")).toHaveTextContent("production");
  const staging = within(dialog).getByRole("option", { name: /^staging/ });
  expect(staging.querySelector(".env-prod")).toBeNull();
});

test("on Welcome only app-wide items show", async () => {
  setup({ project: false });
  const dialog = await openPalette();
  expect(groups(dialog).map(([g]) => g)).toEqual(["Settings", "Commands"]);
  expect(within(dialog).queryByText("Open requests")).toBeNull();
});

test("a failing source is skipped", async () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  setup({ failing: true });
  const dialog = await openPalette();
  await waitFor(() =>
    expect(groups(dialog).map(([g]) => g)).toContain("Environments"),
  );
  expect(groups(dialog).map(([g]) => g)).not.toContain("Broken");
  expect(warn).toHaveBeenCalled();
  warn.mockRestore();
});

test("no results says so", async () => {
  setup();
  const dialog = await openPalette();
  await userEvent.type(within(dialog).getByRole("combobox"), "zzqq");
  expect(within(dialog).getByText('No results for "zzqq"')).toBeInTheDocument();
});

test("5,000 items show the first result within 100 ms", async () => {
  setup({
    items: Array.from({ length: 5000 }, (_, i) => item(`environment ${i}`)),
  });
  performance.clearMeasures();
  const dialog = await openPalette();
  await within(dialog).findAllByRole("option");
  const measure = performance.getEntriesByName("palette-first-result").at(-1);
  expect(measure?.duration ?? Infinity).toBeLessThan(100);
});

test("choosing a Settings section opens Settings on it", async () => {
  const { shell } = setup();
  const dialog = await openPalette();
  await userEvent.type(within(dialog).getByRole("combobox"), "keyboard");
  await userEvent.keyboard("{Enter}");
  await waitFor(() =>
    expect(sectionsOpened).toEqual([{ section: "settings.shortcuts" }]),
  );
  expect(shell.activeModule).toBe("settings");
});

test("the first Settings section shows the Settings shortcut", async () => {
  setup();
  const dialog = await openPalette();
  const appearance = within(dialog).getByRole("option", {
    name: /^Appearance/,
  });
  expect(appearance.querySelector(".shortcut")?.textContent).toMatch(/,/);
  const keys = within(dialog).getByRole("option", { name: /^Keyboard/ });
  expect(keys.querySelector(".shortcut")).toBeNull();
});
