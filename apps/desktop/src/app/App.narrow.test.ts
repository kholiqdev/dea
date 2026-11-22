import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import type { FolderNode } from "$lib/ipc/bindings/FolderNode";
import { shellApi } from "./contributions/shell-api";
import { emptyUiState, shell } from "./shell/shell-state.svelte";
import { fakeIpc, projectInfo } from "./shell/test-support";
import { start } from "./start";

// A window 1024 px wide or narrower, where the sidebars hide.
vi.stubGlobal(
  "matchMedia",
  (query: string): MediaQueryList =>
    ({
      matches: query.includes("max-width: 1024px"),
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList,
);

test("new_request_and_collection_show_the_sidebar_in_a_narrow_window", async () => {
  // The user hid the sidebar in a wide window, and that is what is saved.
  const saved = { requests: { visible: false, width: 300 } };
  const collections: (FolderNode & { kind: "folder" })[] = [];
  const log = fakeIpc({
    project: projectInfo(),
    uiState: { ...emptyUiState(), activeModule: "requests", sidebars: saved },
    answer: (cmd, args) => {
      if (cmd === "request_tree")
        return { collections: structuredClone(collections), errors: [] };
      if (cmd === "create_folder") {
        const path = `${args.parent as string}/new-collection`;
        collections.push({
          kind: "folder",
          path,
          name: args.name as string,
          broken: false,
          requestCount: 0,
          items: [],
        });
        return path;
      }
      if (cmd === "create_request") {
        const folder = collections.find((c) => c.path === args.folder);
        const path = `${args.folder as string}/new-request.yaml`;
        folder?.items.push({
          kind: "request",
          path,
          name: args.name as string,
          method: "GET",
          url: "",
        });
        if (folder) folder.requestCount++;
        return path;
      }
      return undefined;
    },
  });
  await start(document.body.appendChild(document.createElement("div")));
  expect(
    await screen.findByRole("button", { name: "Show sidebar" }),
  ).toBeInTheDocument();
  expect(screen.queryByRole("tree", { name: "Collections" })).toBeNull();

  await shellApi().runCommand("requests.newCollection");

  const field = await screen.findByRole("textbox", { name: "Name" });
  await waitFor(() => expect(field).toHaveFocus());
  expect(field).toHaveValue("New collection");
  expect(
    screen.getByRole("button", { name: "Hide sidebar" }),
  ).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");

  // Hidden again, and New request shows it the same way.
  await userEvent.click(screen.getByRole("button", { name: "Hide sidebar" }));
  await waitFor(() =>
    expect(screen.queryByRole("tree", { name: "Collections" })).toBeNull(),
  );
  await shellApi().runCommand("requests.newRequest");
  const named = await screen.findByRole("textbox", { name: "Name" });
  await waitFor(() => expect(named).toHaveFocus());
  expect(named).toHaveValue("New request");

  // Shown for now: what is saved stays as it was.
  expect(shell.snapshot().sidebars).toEqual(saved);
  await new Promise((resolve) => setTimeout(resolve, 400));
  for (const { state } of log.savedUiStates)
    expect(state.sidebars).toEqual(saved);
});
