import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import { closeQuestion } from "../prompts/confirm-close.svelte";
import { Tabs } from "../tabs/tabs.svelte";
import ShellView from "./Shell.svelte";
import { Shell } from "./shell-state.svelte";
import {
  fakeIpc,
  openProject,
  projectInfo,
  railModule,
  registryWith,
  view,
  type FakeIpcOptions,
} from "./test-support";
import { windowReload } from "./window-reload";

function setup(info: ProjectInfo, ipc: FakeIpcOptions = {}) {
  const log = fakeIpc(ipc);
  const reload = vi.spyOn(windowReload, "run").mockImplementation(() => {});
  const registry = registryWith(
    railModule("requests", 1, {
      tabTypes: [
        {
          id: "requests.request",
          title: "Request",
          icon: "x",
          view: view("r"),
        },
      ],
    }),
  );
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  render(ShellView, { registry, shell, tabs, project: openProject(info) });
  return { log, reload, tabs };
}

test("a window whose folder is gone offers Locate and Close project", async () => {
  const { log, reload } = setup(
    projectInfo({ missing: true, environments: [] }),
    { picked: "/work/harbor-api-moved" },
  );
  const banner = screen.getByText(/Folder not found/).closest(".banner");
  expect(banner).toHaveTextContent(
    "Folder not found: /work/harbor-api. It was moved, renamed or deleted outside Dea.",
  );
  await userEvent.click(screen.getByRole("button", { name: "Locate" }));
  await waitFor(() => expect(reload).toHaveBeenCalledOnce());
  expect(log.calls).toContainEqual({
    cmd: "open_project",
    args: { root: "/work/harbor-api-moved" },
  });

  await userEvent.click(screen.getByRole("button", { name: "Close project" }));
  await waitFor(() => expect(reload).toHaveBeenCalledTimes(2));
  expect(log.calls.map((c) => c.cmd)).toContain("close_project");
});

test("a dea.yaml with conflict markers is named above the work area", () => {
  setup(
    projectInfo({
      errors: [
        {
          path: "dea.yaml",
          line: null,
          message: "unresolved merge conflict",
          mergeConflict: true,
        },
      ],
    }),
  );
  expect(
    screen.getByText(/has Git conflict markers/).closest(".banner"),
  ).toHaveTextContent(
    "dea.yaml has Git conflict markers. Resolve them in your editor; Dea reloads the file when it is clean.",
  );
});

test("Close project waits while a tab has unsaved changes", async () => {
  const { log, reload, tabs } = setup(projectInfo());
  const id = tabs.open("requests.request", "Draft", null);
  tabs.setDirty(id, true);
  flushSync();
  await userEvent.click(screen.getByRole("button", { name: /harbor-api/ }));
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Close project" }),
  );
  expect(
    await screen.findByText("Save or close the tabs with changes first"),
  ).toBeInTheDocument();
  expect(log.calls.map((c) => c.cmd)).not.toContain("close_project");
  expect(reload).not.toHaveBeenCalled();
});
