import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import { toasts } from "../prompts/toasts.svelte";
import { closeQuestion } from "../prompts/confirm-close.svelte";
import { Tabs } from "../tabs/tabs.svelte";
import { shellModule } from "./shell-module";
import ShellView from "./Shell.svelte";
import { Shell } from "./shell-state.svelte";
import {
  fakeIpc,
  openProject,
  railModule,
  registryWith,
  view,
} from "./test-support";

beforeEach(() => {
  fakeIpc();
});

function setup(...modules: Parameters<typeof registryWith>) {
  const registry = registryWith(...modules);
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  render(ShellView, { registry, shell, tabs, project: openProject() });
  return { registry, shell, tabs };
}

test("rail shows only registered modules and opens them", async () => {
  setup(railModule("requests", 1), railModule("database", 2));
  const rail = screen.getByRole("navigation", { name: "Modules" });
  const buttons = [...rail.querySelectorAll("button[data-module]")].map((b) =>
    b.getAttribute("data-module"),
  );
  expect(buttons).toEqual(["requests", "database"]);
  await userEvent.click(screen.getByRole("button", { name: "Database" }));
  expect(await screen.findByText("database view")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Database" })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("Mod+2 opens Database", async () => {
  setup(railModule("requests", 1), railModule("database", 2));
  await userEvent.keyboard("{Control>}2{/Control}");
  expect(await screen.findByText("database view")).toBeInTheDocument();
});

test("a shortcut turned off in settings does nothing", async () => {
  const { shell } = setup(railModule("requests", 1));
  await settings.update({ shortcuts: { "requests.open": null } });
  await userEvent.keyboard("{Control>}1{/Control}");
  expect(shell.activeModule).toBeNull();
  await settings.update({ shortcuts: { "requests.open": "Mod+Shift+R" } });
  await userEvent.keyboard("{Control>}{Shift>}r{/Shift}{/Control}");
  await waitFor(() => expect(shell.activeModule).toBe("requests"));
  await settings.update({ shortcuts: {} });
});

test("the rail expands to show names and shortcuts", async () => {
  const { shell } = setup(railModule("requests", 1));
  await userEvent.click(
    screen.getByRole("button", { name: "Show module names" }),
  );
  expect(shell.railExpanded).toBe(true);
  expect(screen.getByText("Ctrl+1")).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "Collapse the rail" }),
  );
  expect(shell.railExpanded).toBe(false);
});

test("every shell control is reachable with Tab", async () => {
  const { shell } = setup(
    railModule("requests", 1, { sidebar: view("requests sidebar") }),
    railModule("database", 2),
    {
      id: "tunnels",
      title: "Tunnels",
      statusItems: [
        {
          id: "tunnels.count",
          align: "end",
          order: 1,
          view: () => import("./test-fixtures/StatusButton.svelte"),
        },
      ],
    },
  );
  await shell.openModule("requests");
  const status = await screen.findByRole("button", { name: "status" });
  const controls = [
    ...document.querySelectorAll<HTMLElement>("[data-shell-control]"),
  ];
  expect(controls.length).toBeGreaterThanOrEqual(5);
  const reached: Element[] = [];
  document.body.focus();
  for (let i = 0; i < controls.length + 5; i++) {
    await userEvent.tab();
    if (document.activeElement) reached.push(document.activeElement);
  }
  for (const c of controls) expect(reached, c.outerHTML).toContain(c);
  // A control inside a status item is reached too, after the shell's own.
  expect(reached).toContain(status);
  expect(reached.indexOf(status)).toBeGreaterThan(
    Math.max(...controls.map((c) => reached.indexOf(c))),
  );
  // The title bar comes first.
  expect(controls[0]?.closest("header")).not.toBeNull();
});

test("the sidebar can be hidden and resized from the keyboard", async () => {
  const { shell } = setup(
    railModule("requests", 1, { sidebar: view("requests sidebar") }),
  );
  await shell.openModule("requests");
  expect(await screen.findByText("requests sidebar")).toBeInTheDocument();
  const handle = screen.getByRole("separator", { name: "Resize the sidebar" });
  handle.focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(shell.sidebar("requests").width).toBe(296);
  await userEvent.click(screen.getByRole("button", { name: "Hide sidebar" }));
  expect(screen.queryByText("requests sidebar")).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Show sidebar" }));
  expect(await screen.findByText("requests sidebar")).toBeInTheDocument();
});

test("status bar renders registered items only", async () => {
  const { registry } = setup({
    id: "tunnels",
    title: "Tunnels",
    statusItems: [
      { id: "tunnels.count", align: "end", order: 1, view: view("2 tunnels") },
      { id: "tunnels.route", align: "start", order: 1, view: view("route") },
    ],
  });
  const footer = screen.getByRole("contentinfo");
  expect(await screen.findByText("2 tunnels")).toBeInTheDocument();
  expect(footer).toHaveTextContent("route");
  registry.setEnabled("tunnels", false);
  await waitFor(() => expect(footer).not.toHaveTextContent("2 tunnels"));
});

test("the title bar names the project", () => {
  setup();
  expect(screen.getByRole("banner")).toHaveTextContent("harbor-api");
});

test("switching modules replaces the view", async () => {
  const { shell } = setup(railModule("requests", 1), railModule("database", 2));
  await shell.openModule("requests");
  expect(await screen.findByText("requests view")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Database" }));
  expect(await screen.findByText("database view")).toBeInTheDocument();
  expect(screen.queryByText("requests view")).not.toBeInTheDocument();
});

test("shortcuts do nothing while a dialog is open", async () => {
  const { shell } = setup(railModule("requests", 1), railModule("database", 2));
  const answer = closeQuestion.ask({
    id: "t",
    tabType: "requests.request",
    title: "Draft",
    params: null,
    module: "requests",
    dirty: true,
  });
  const dialog = await screen.findByRole("dialog", {
    name: "Save changes to Draft?",
  });
  // Everything behind the dialog is out of reach.
  expect(document.querySelector(".app")).toHaveAttribute("inert");

  await userEvent.keyboard("{Control>}2{/Control}");
  expect(shell.activeModule).toBeNull();
  expect(dialog).toBeInTheDocument();

  await userEvent.keyboard("{Escape}");
  expect(await answer).toBe("cancel");
  expect(document.querySelector(".app")).not.toHaveAttribute("inert");
  await userEvent.keyboard("{Control>}2{/Control}");
  await waitFor(() => expect(shell.activeModule).toBe("database"));
});

test("marking a tab dirty keeps its view mounted", async () => {
  let mounts = 0;
  const counted = view("notes view");
  const loader = () =>
    counted().then(({ default: component }) => ({
      default: ((anchor: never, props: Record<string, unknown>) => {
        mounts++;
        return component(anchor, props);
      }) as typeof component,
    }));
  const { tabs } = setup(
    railModule("notes", 1, {
      tabTypes: [
        { id: "notes.note", title: "Note", icon: "file", view: loader },
      ],
    }),
  );
  const id = tabs.open("notes.note", "Note", null);
  expect(await screen.findByText("notes view")).toBeInTheDocument();
  expect(mounts).toBe(1);
  tabs.setDirty(id, true);
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(screen.getByText("notes view")).toBeInTheDocument();
  expect(mounts).toBe(1);
});

function setupWithShellCommands(...modules: Parameters<typeof registryWith>) {
  const registry = registryWith(...modules);
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  const project = openProject();
  registry.register(shellModule(shell, tabs, project));
  render(ShellView, { registry, shell, tabs, project });
  return { registry, shell, tabs };
}

const noteModule = () =>
  railModule("notes", 1, {
    tabTypes: [
      { id: "notes.note", title: "Note", icon: "file", view: view("notes") },
    ],
  });

test("Mod+S saves the active tab through its type's save handler", async () => {
  const saved: string[] = [];
  const { tabs } = setupWithShellCommands(noteModule());
  tabs.setSaveHandler("notes.note", (id) => {
    saved.push(id);
    return Promise.resolve();
  });
  const id = tabs.open("notes.note", "Note", null);
  await userEvent.keyboard("{Control>}s{/Control}");
  await waitFor(() => expect(saved).toEqual([id]));
});

test("Mod+S does nothing for a tab type without a save handler", async () => {
  const { tabs } = setupWithShellCommands(noteModule());
  tabs.open("notes.note", "Note", null);
  await userEvent.keyboard("{Control>}s{/Control}");
  expect(tabs.tabs).toHaveLength(1);
});

test("a toast action runs from its chord while no field has focus", async () => {
  setup(noteModule());
  let undone = 0;
  const id = toasts.show("Deleted Update status", {
    action: { label: "Undo", run: () => undone++, chord: "Mod+Z" },
    timeout: 10_000,
  });
  expect(await screen.findByText("Ctrl")).toBeInTheDocument();
  await userEvent.keyboard("{Control>}z{/Control}");
  expect(undone).toBe(1);
  expect(toasts.items.some((t) => t.id === id)).toBe(false);
});

test("a toast chord is ignored while typing", async () => {
  setup(noteModule());
  let undone = 0;
  const id = toasts.show("Deleted Update status", {
    action: { label: "Undo", run: () => undone++, chord: "Mod+Z" },
    timeout: 10_000,
  });
  const field = document.createElement("input");
  document.body.append(field);
  field.focus();
  await userEvent.keyboard("{Control>}z{/Control}");
  expect(undone).toBe(0);
  field.remove();
  toasts.dismiss(id);
});

test("window environment tabs follow the active environment", async () => {
  const registry = registryWith(
    railModule("notes", 1, {
      tabTypes: [
        {
          id: "notes.note",
          title: "Note",
          icon: "file",
          view: view("notes"),
          environment: "window",
        },
      ],
    }),
  );
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  const project = openProject();
  await project.setEnvironment("staging");
  render(ShellView, { registry, shell, tabs, project });
  tabs.open("notes.note", "Note", null);
  const pill = await screen.findByText("staging", { selector: ".tab-env" });
  expect(pill).not.toHaveClass("env-prod");
  await project.setEnvironment("production");
  const prod = await screen.findByText("production", { selector: ".tab-env" });
  expect(prod).toHaveClass("env-prod");
});
