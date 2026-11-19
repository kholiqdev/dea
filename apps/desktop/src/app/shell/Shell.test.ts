import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import ShellView from "./Shell.svelte";
import { Shell } from "./shell-state.svelte";
import { fakeIpc, railModule, registryWith, view } from "./test-support";

beforeEach(() => {
  fakeIpc();
});

function setup(...modules: Parameters<typeof registryWith>) {
  const registry = registryWith(...modules);
  const shell = new Shell(registry);
  render(ShellView, { registry, shell });
  return { registry, shell };
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

test("the title bar says no project is open", () => {
  setup();
  expect(screen.getByRole("banner")).toHaveTextContent("No project open");
});

test("switching modules replaces the view", async () => {
  const { shell } = setup(railModule("requests", 1), railModule("database", 2));
  await shell.openModule("requests");
  expect(await screen.findByText("requests view")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Database" }));
  expect(await screen.findByText("database view")).toBeInTheDocument();
  expect(screen.queryByText("requests view")).not.toBeInTheDocument();
});
