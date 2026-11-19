import { screen, within } from "@testing-library/svelte";
import { expect, test } from "vitest";
import { fakeIpc, projectInfo } from "./shell/test-support";
import { start } from "./start";
import { tabs } from "./tabs/tabs.svelte";

test("a window with a project shows its modules and the saved environment", async () => {
  const log = fakeIpc({
    project: projectInfo(),
    uiState: {
      version: 1,
      activeModule: null,
      activeEnvironment: "staging",
      railExpanded: false,
      sidebars: {},
      tabs: [],
      activeTab: null,
      closedTabs: [],
    },
  });
  const target = document.body.appendChild(document.createElement("div"));
  await start(target);

  const rail = screen.getByRole("navigation", { name: "Modules" });
  for (const name of [
    "Requests",
    "Database",
    "Terminal",
    "Mobile",
    "Vault",
    "Settings",
  ])
    expect(within(rail).getByRole("button", { name })).toBeInTheDocument();
  const bar = screen.getByRole("banner");
  expect(
    within(bar).getByRole("button", { name: /harbor-api/ }),
  ).toBeInTheDocument();
  expect(
    within(bar).getByRole("button", { name: /staging/ }),
  ).toBeInTheDocument();
  expect(log.calls).toContainEqual({
    cmd: "get_ui_state",
    args: { scope: "scope-1", slot: 0 },
  });
  expect(log.calls).toContainEqual({
    cmd: "set_active_environment",
    args: { name: "staging" },
  });
  // The first module on the rail opens.
  expect(
    await screen.findByRole("heading", { name: "Requests" }),
  ).toBeInTheDocument();
});

// Uses the window the test above started.
test("the environment editor opens in a tab and settles", async () => {
  tabs.open("project.environments", "Environments", null);
  expect(
    await screen.findByRole("navigation", { name: "Environments" }),
  ).toBeInTheDocument();
  expect(tabs.active?.dirty).toBe(false);
});
