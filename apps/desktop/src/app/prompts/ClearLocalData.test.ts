import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";
import type { UiState } from "$lib/ipc/bindings/UiState";
import { fakeIpc, projectInfo, type IpcLog } from "../shell/test-support";
import { shell } from "../shell/shell-state.svelte";
import { start } from "../start";
import { tabs } from "../tabs/tabs.svelte";
import ClearLocalData from "./ClearLocalData.svelte";

let log: IpcLog;

test("Clear local data asks first and starts on Cancel", async () => {
  const oncancel = vi.fn();
  const onclear = vi.fn();
  const { unmount } = render(ClearLocalData, {
    name: "harbor-api",
    oncancel,
    onclear,
  });
  const dialog = screen.getByRole("dialog", {
    name: "Clear local data for harbor-api?",
  });
  expect(dialog).toHaveTextContent(
    "Removes the window layout and open tabs Dea keeps for this project on this device. Project files are not touched.",
  );
  expect(within(dialog).getByRole("button", { name: /Cancel/ })).toHaveFocus();
  expect(within(dialog).getByRole("button", { name: "Clear" })).toHaveClass(
    "danger",
  );

  await userEvent.keyboard("{Enter}");
  expect(oncancel).toHaveBeenCalledOnce();
  await userEvent.click(within(dialog).getByRole("button", { name: "Clear" }));
  expect(onclear).toHaveBeenCalledOnce();
  unmount();
});

test("clearing resets tabs and layout and the old state is not saved again", async () => {
  const saved: UiState = {
    version: 1,
    activeModule: null,
    activeEnvironment: "staging",
    railExpanded: true,
    sidebars: {},
    tabs: [
      {
        id: "env-tab",
        tabType: "project.environments",
        title: "Environments",
        params: null,
      },
    ],
    activeTab: "env-tab",
    closedTabs: [],
  };
  // A slow disk: a save of the old layout still waiting must not land
  // after the clear.
  log = fakeIpc({
    project: projectInfo(),
    uiState: saved,
    clearTakes: 500,
  });
  await start(document.body.appendChild(document.createElement("div")));
  expect(tabs.tabs).toHaveLength(1);

  // A layout change waits to be saved with the old tabs.
  shell.railExpanded = false;
  flushSync();

  const bar = screen.getByRole("banner");
  await userEvent.click(
    within(bar).getByRole("button", { name: /harbor-api/ }),
  );
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Clear local data…" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Clear" }));

  expect(await screen.findByText("Local data cleared")).toBeInTheDocument();
  expect(tabs.tabs).toHaveLength(0);
  expect(shell.railExpanded).toBe(false);
  // The environment is not layout: it stays.
  expect(
    within(bar).getByRole("button", { name: /staging/ }),
  ).toBeInTheDocument();

  const cleared = log.calls.findIndex((c) => c.cmd === "clear_local_data");
  expect(cleared).toBeGreaterThan(-1);
  await new Promise((resolve) => setTimeout(resolve, 400));
  const after = log.calls
    .slice(cleared)
    .filter((c) => c.cmd === "save_ui_state")
    .map((c) => (c.args as { state: UiState }).state);
  // The empty layout is saved; nothing from before the clear lands after it.
  expect(after.length).toBeGreaterThan(0);
  for (const state of after) expect(state.tabs).toEqual([]);
});

// Uses the window the test above started.
test("clearing waits while a tab has unsaved changes", async () => {
  const id = tabs.open("project.environments", "Environments", null);
  tabs.setDirty(id, true);
  const clears = () => log.calls.filter((c) => c.cmd === "clear_local_data");
  const before = clears().length;

  const bar = screen.getByRole("banner");
  await userEvent.click(
    within(bar).getByRole("button", { name: /harbor-api/ }),
  );
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Clear local data…" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Clear" }));

  expect(
    await screen.findByText("Save or close the tabs with changes first"),
  ).toBeInTheDocument();
  expect(tabs.tabs.map((t) => t.id)).toEqual([id]);
  expect(clears()).toHaveLength(before);
});
