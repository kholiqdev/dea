// The palette with 5,000 items (F1-FON-11). It measures wall time, so it
// runs in the perf project: alone, after the other tests.

import { render, waitFor, within, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
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

beforeEach(() => {
  // The palette's open state belongs to the window, so it outlives a test.
  menus.palette = false;
  fakeIpc();
});

function setup(items: PaletteItem[]): void {
  const registry = registryWith(
    railModule("requests", 1, {
      searchSources: [
        {
          id: "requests.environments",
          group: "Environments",
          order: 60,
          source: () => Promise.resolve({ default: () => items }),
        },
      ],
    }),
  );
  const shell = new Shell(registry);
  const tabs = new Tabs(registry, shell, { confirmClose: closeQuestion.ask });
  const project = openProject(projectInfo());
  registry.register(shellModule(shell, tabs, project));
  registry.setProjectOpen(true);
  render(ShellView, { registry, shell, tabs, project });
}

test("5,000 items show the first result within 100 ms", async () => {
  setup(
    Array.from({ length: 5000 }, (_, i) => ({
      id: `environment ${i}`,
      title: `environment ${i}`,
      open: () => {},
    })),
  );
  performance.clearMeasures();
  await userEvent.keyboard("{Control>}k{/Control}");
  const dialog = await screen.findByRole("dialog", { name: "Search" });
  await within(dialog).findAllByRole("option");
  await waitFor(() =>
    expect(performance.getEntriesByName("palette-all-results")).toHaveLength(1),
  );
  const measure = performance.getEntriesByName("palette-all-results").at(-1);
  expect(measure?.duration ?? Infinity).toBeLessThan(100);
});
