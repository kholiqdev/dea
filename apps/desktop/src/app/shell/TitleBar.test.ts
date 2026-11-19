import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { beforeEach, expect, test } from "vitest";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import { closeQuestion } from "../prompts/confirm-close.svelte";
import { Tabs } from "../tabs/tabs.svelte";
import ShellView from "./Shell.svelte";
import { Shell } from "./shell-state.svelte";
import { shellModule } from "./shell-module";
import {
  fakeIpc,
  openProject,
  projectInfo,
  railModule,
  registryWith,
  type IpcLog,
} from "./test-support";

let ipc: IpcLog;
beforeEach(() => {
  ipc = fakeIpc();
});

function setup(info: ProjectInfo | null = projectInfo()) {
  const registry = registryWith(
    railModule("requests", 1),
    {
      id: "settings",
      title: "Settings",
      scope: "app",
      rail: { icon: "x", group: "bottom", order: 2 },
      view: railModule("settings", 9).view,
      commands: [
        {
          id: "settings.open",
          title: "Open Settings",
          action: { type: "openModule", module: "settings" },
        },
      ],
      shortcuts: [{ command: "settings.open", chord: "Mod+," }],
    },
    {
      id: "project",
      title: "Welcome",
      scope: "app",
      view: railModule("welcome", 8).view,
    },
  );
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  const project = openProject(info);
  registry.register(shellModule(shell, tabs, project));
  registry.setProjectOpen(info !== null);
  render(ShellView, { registry, shell, tabs, project });
  return { registry, shell, project };
}

test("the title bar shows the project and its environment", () => {
  setup();
  const bar = screen.getByRole("banner");
  expect(
    within(bar).getByRole("button", { name: /harbor-api/ }),
  ).toBeInTheDocument();
  expect(within(bar).getByRole("button", { name: /local/ })).toHaveAttribute(
    "title",
    "Environment for requests",
  );
});

test("switching the environment updates the title bar and is saved", async () => {
  const { project } = setup();
  await userEvent.click(screen.getByRole("button", { name: /local/ }));
  await userEvent.click(
    screen.getByRole("menuitemradio", { name: /production/ }),
  );
  expect(project.environment?.name).toBe("production");
  expect(ipc.calls).toContainEqual({
    cmd: "set_active_environment",
    args: { name: "production" },
  });
  const bar = screen.getByRole("banner");
  expect(
    within(bar).getByText("production", { selector: ".prod-label" }),
  ).toBeInTheDocument();
  expect(document.querySelector(".app")).toHaveAttribute(
    "data-env",
    "production",
  );
});

test("Mod+E opens the environment picker and arrows plus Enter choose", async () => {
  const { project } = setup();
  await userEvent.keyboard("{Control>}e{/Control}");
  const menu = await screen.findByRole("menu", { name: "Environments" });
  expect(
    within(menu).getByRole("menuitemradio", { name: /local/ }),
  ).toHaveFocus();
  await userEvent.keyboard("{ArrowDown}{Enter}");
  await waitFor(() => expect(project.environment?.name).toBe("staging"));
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /staging/ })).toHaveFocus();
});

test("Escape closes the picker and gives focus back", async () => {
  setup();
  const trigger = screen.getByRole("button", { name: /local/ });
  await userEvent.click(trigger);
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

test("a window without a project shows Welcome and no rail", async () => {
  const { shell } = setup(null);
  await shell.openModule("project");
  flushSync();
  expect(
    screen.queryByRole("navigation", { name: "Modules" }),
  ).not.toBeInTheDocument();
  expect(await screen.findByText("welcome view")).toBeInTheDocument();
  expect(screen.getByRole("banner")).toHaveTextContent("No project open");
});

test("project-scoped modules and their shortcuts are off without a project", async () => {
  const { shell } = setup(null);
  await shell.openModule("project");
  await userEvent.keyboard("{Control>}1{/Control}");
  expect(shell.activeModule).toBe("project");
  await userEvent.keyboard("{Control>},{/Control}");
  expect(await screen.findByText("settings view")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(await screen.findByText("welcome view")).toBeInTheDocument();
});
