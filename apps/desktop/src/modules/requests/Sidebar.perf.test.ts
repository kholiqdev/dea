// The sidebar filter with 5,000 requests (F1-REQ-01 #3). It measures wall
// time, so it runs in the perf project: alone, after the other tests.

import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import { FakeProject } from "./fake-project.svelte";
import CollectionsSidebar from "./components/CollectionsSidebar.svelte";
import { resetTreeState } from "./state/tree.svelte";
import { bigTree, fakeRequestsIpc } from "./test-support";

beforeEach(() => {
  performance.clearMarks();
  performance.clearMeasures();
  resetTreeState();
  setProjectApi(new FakeProject([{ name: "local", production: false }]));
  setShellApi({
    openTab: () => "t",
    findTab: () => undefined,
    updateTab: () => {},
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    activeTab: () => null,
    isTabDirty: () => false,
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    toast: () => 0,
    dismissToast: () => {},
    confirmingSend: false,
    runCommand: () => Promise.resolve(),
  } satisfies ShellApi);
});

test("5,000 requests filter within 100 ms", async () => {
  fakeRequestsIpc({ tree: bigTree(5000) });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  const query = "shipment 4999";
  await userEvent.type(
    screen.getByRole("searchbox", { name: "Filter collections" }),
    query,
  );
  // Every keystroke counts; the first ones match all 5,000 requests.
  await waitFor(() =>
    expect(performance.getEntriesByName("requests-filter")).toHaveLength(
      query.length,
    ),
  );
  const entries = performance.getEntriesByName("requests-filter");
  expect(Math.max(...entries.map((e) => e.duration))).toBeLessThan(100);
  expect(screen.getAllByRole("treeitem").map((r) => r.textContent)).toContain(
    "GETshipment 4999",
  );
});
