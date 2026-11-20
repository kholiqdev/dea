import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { FakeProject } from "./fake-project.svelte";
import CollectionsSidebar from "./components/CollectionsSidebar.svelte";
import newCollection from "./intents/new-collection";
import newRequest from "./intents/new-request";
import { resetTreeState, treeState } from "./state/tree.svelte";
import { fakeRequestsIpc, folder, leaf } from "./test-support";

interface OpenTab {
  id: string;
  type: string;
  title: string;
  params: { path: string };
  dirty: boolean;
}

interface ShownToast {
  text: string;
  action?: { label: string; run: () => void; chord?: string };
  timeout?: number;
}

let tabs: OpenTab[];
let modules: string[];
const toasts = {
  items: [] as ShownToast[],
  get last(): ShownToast {
    const toast = this.items.at(-1);
    if (!toast) throw new Error("No toast was shown");
    return toast;
  },
};

function shipments(): RequestTreeInfo {
  return {
    collections: [
      folder("requests/shipments", "Shipments", [
        leaf(
          "requests/shipments/list.yaml",
          "List shipments",
          "GET",
          "https://harbor.test/shipments",
        ),
        leaf(
          "requests/shipments/update-status.yaml",
          "Update status",
          "PATCH",
          "https://harbor.test/shipments/{{id}}",
        ),
        leaf(
          "requests/shipments/create.yaml",
          "Create shipment",
          "POST",
          "https://harbor.test/shipments",
        ),
      ]),
      folder("requests/tracking", "Tracking", [
        folder("requests/tracking/carriers", "Carriers", [
          leaf("requests/tracking/carriers/dhl.yaml", "DHL"),
        ]),
        leaf("requests/tracking/events.yaml", "Tracking events"),
      ]),
    ],
    errors: [],
  };
}

function openTab(type: string, title: string, path: string): OpenTab {
  const tab = {
    id: `tab-${tabs.length + 1}`,
    type,
    title,
    params: { path },
    dirty: false,
  };
  tabs.push(tab);
  return tab;
}

beforeEach(() => {
  resetTreeState();
  // Both collections start open; Carriers stays closed.
  treeState.expanded.add("requests/shipments");
  treeState.expanded.add("requests/tracking");
  tabs = [];
  modules = [];
  toasts.items = [];
  setProjectApi(new FakeProject([{ name: "local", production: false }]));
  setShellApi({
    openTab: (type, title, params) =>
      openTab(type, title, (params as { path: string }).path).id,
    findTab: (type, match) =>
      tabs.find((t) => t.type === type && (!match || match(t.params)))?.id,
    updateTab: (id, patch) => {
      const tab = tabs.find((t) => t.id === id);
      if (!tab) return;
      if (patch.title !== undefined) tab.title = patch.title;
      if (patch.params !== undefined)
        tab.params = patch.params as { path: string };
    },
    activateTab: (id) => tabs.some((t) => t.id === id),
    // Like the shell: a tab with unsaved changes asks, and here stays open.
    closeTab: (id) => {
      const tab = tabs.find((t) => t.id === id);
      if (!tab || tab.dirty) return Promise.resolve(false);
      tabs = tabs.filter((t) => t !== tab);
      return Promise.resolve(true);
    },
    setTabDirty: (id, dirty) => {
      const tab = tabs.find((t) => t.id === id);
      if (tab) tab.dirty = dirty;
    },
    setSaveHandler: () => {},
    openModule: (id) => {
      modules.push(id);
      return Promise.resolve();
    },
    toast: (text, opts) => {
      toasts.items.push({ text, ...opts });
      return toasts.items.length;
    },
    dismissToast: () => {},
    runCommand: () => Promise.resolve(),
  });
});

afterEach(() => {
  vi.useRealTimers();
});

// A row's accessible name is its method or count plus its name.
const named = (name: string) => (accessible: string) =>
  accessible
    .replace(/^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)/, "")
    .replace(/\d+$/, "") === name;
const row = (name: string) =>
  screen.getByRole("treeitem", { name: named(name) });
const rowNames = () =>
  screen.getAllByRole("treeitem").map((r) => r.textContent);
const calls = (ipc: { calls: [string, unknown][] }, cmd: string) =>
  ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);

async function showTree(): Promise<HTMLElement> {
  render(CollectionsSidebar);
  return screen.findByRole("tree", { name: "Collections" });
}

async function openMenu(name: string): Promise<HTMLElement> {
  await userEvent.pointer({ keys: "[MouseRight]", target: row(name) });
  return screen.findByRole("menu");
}

// jsdom has no layout: rows sit 28 px apart from the top of the list.
const ROW = 28;
type Place = "before" | "middle" | "after";

function yOf(el: HTMLElement, place: Place): number {
  const index = screen.getAllByRole("treeitem").indexOf(el);
  return index * ROW + { before: 3, middle: 14, after: 25 }[place];
}

const pointer = (y: number) => ({
  button: 0,
  pointerId: 1,
  clientX: 60,
  clientY: y,
});

/** A press on a row that moves past the drag threshold. */
async function press(from: HTMLElement): Promise<void> {
  const y = yOf(from, "middle");
  await fireEvent.pointerDown(from, pointer(y));
  await fireEvent.pointerMove(from, pointer(y + 5));
}

async function hover(to: HTMLElement, place: Place): Promise<number> {
  const y = yOf(to, place);
  await fireEvent.pointerMove(to, pointer(y));
  return y;
}

async function drag(from: HTMLElement, to: HTMLElement, place: Place) {
  await press(from);
  const y = await hover(to, place);
  await fireEvent.pointerUp(to, pointer(y));
}

test("delete asks first, then offers Undo for 10 seconds", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  const tab = openTab(
    "requests.request",
    "Update status",
    "requests/shipments/update-status.yaml",
  );
  tab.dirty = true;
  await openMenu("Update status");
  await userEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  const dialog = screen.getByRole("dialog", { name: "Delete Update status?" });
  expect(dialog).toHaveTextContent(
    "The request file is removed from the project. You can undo this for 10 seconds.",
  );
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  await waitFor(() =>
    expect(toasts.last).toMatchObject({
      text: "Deleted Update status",
      action: { label: "Undo", chord: "Mod+Z" },
      timeout: 10_000,
    }),
  );
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(calls(ipc, "delete_request_item")).toEqual([
    { path: "requests/shipments/update-status.yaml" },
  ]);
  // Its tab closes without asking: the file can come back.
  expect(tabs).toEqual([]);
  await waitFor(() => expect(rowNames()).not.toContain("PATCHUpdate status"));
  toasts.last.action?.run();
  await waitFor(() =>
    expect(ipc.calls).toContainEqual(["restore_request_item", { id: 1 }]),
  );
  await waitFor(() => expect(rowNames()).toContain("PATCHUpdate status"));
});

test("cancel keeps the item and gives the tree the focus", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  const tree = await showTree();
  await openMenu("Update status");
  await userEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  await userEvent.keyboard("{Enter}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(calls(ipc, "delete_request_item")).toEqual([]);
  await waitFor(() => expect(tree).toHaveFocus());
});

test("deleting a folder counts its requests and closes their tabs", async () => {
  fakeRequestsIpc({ tree: shipments() });
  await showTree();
  openTab("requests.request", "DHL", "requests/tracking/carriers/dhl.yaml");
  openTab("requests.folder", "Tracking", "requests/tracking");
  openTab("requests.request", "List shipments", "requests/shipments/list.yaml");
  await openMenu("Tracking");
  await userEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  const dialog = screen.getByRole("dialog", { name: "Delete Tracking?" });
  expect(dialog).toHaveTextContent(
    "The folder and the 2 requests in it are removed from the project. You can undo this for 10 seconds.",
  );
  await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(toasts.last.text).toBe("Deleted Tracking"));
  expect(tabs.map((t) => t.title)).toEqual(["List shipments"]);
});

test("restore failure names the item", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  ipc.failures.set("restore_request_item", {
    kind: "missing",
    message: "the deleted item is no longer kept",
  });
  await showTree();
  await openMenu("Update status");
  await userEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  await userEvent.click(screen.getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(toasts.last.text).toBe("Deleted Update status"));
  toasts.last.action?.run();
  await waitFor(() =>
    expect(toasts.last.text).toBe(
      "Update status could not be restored: the deleted item is no longer kept",
    ),
  );
});

test("menus offer what fits the item", async () => {
  fakeRequestsIpc({ tree: shipments() });
  await showTree();
  const labels = (menu: HTMLElement) =>
    within(menu)
      .getAllByRole("menuitem")
      .map((i) => i.textContent.trim());
  expect(labels(await openMenu("Shipments"))).toEqual([
    "New request",
    "New folder",
    "Rename",
    "Duplicate",
    "Move to…",
    "Collection settings",
    "Delete…",
  ]);
  await userEvent.keyboard("{Escape}");
  expect(labels(await openMenu("Carriers"))).toEqual([
    "New request",
    "New folder",
    "Rename",
    "Duplicate",
    "Move to…",
    "Folder settings",
    "Delete…",
  ]);
  await userEvent.keyboard("{Escape}");
  expect(labels(await openMenu("Tracking events"))).toEqual([
    "Rename",
    "Duplicate",
    "Move to…",
    "Delete…",
  ]);
  await userEvent.keyboard("{Escape}");
  // The list's empty space below the rows.
  const tree = screen.getByRole("tree", { name: "Collections" });
  await userEvent.pointer({
    keys: "[MouseRight]",
    target: tree.parentElement!.parentElement!,
  });
  expect(labels(await screen.findByRole("menu"))).toEqual(["New collection"]);
});

test("the active row's More actions button and Shift+F10 open its menu", async () => {
  fakeRequestsIpc({ tree: shipments() });
  const tree = await showTree();
  await userEvent.click(row("Tracking events"));
  await userEvent.click(screen.getByRole("button", { name: "More actions" }));
  expect(
    screen.getByRole("menu", { name: "Tracking events" }),
  ).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  tree.focus();
  await userEvent.keyboard("{ArrowUp}{Shift>}{F10}{/Shift}");
  expect(screen.getByRole("menu", { name: "Carriers" })).toBeInTheDocument();
  expect(screen.getByRole("menuitem", { name: "New request" })).toHaveFocus();
  await userEvent.keyboard("{Escape}");
  expect(tree).toHaveFocus();
});

test("Collection settings opens the folder tab", async () => {
  fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await openMenu("Shipments");
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Collection settings" }),
  );
  expect(tabs).toMatchObject([
    {
      type: "requests.folder",
      title: "Shipments",
      params: { path: "requests/shipments" },
    },
  ]);
});

test("new request starts renaming inline", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  const tree = await showTree();
  await userEvent.click(screen.getByRole("button", { name: "New request" }));
  const input = await screen.findByRole("textbox", { name: "Name" });
  expect(calls(ipc, "create_request")).toEqual([
    { folder: "requests/shipments", name: "New request" },
  ]);
  expect(input).toHaveFocus();
  expect(input).toHaveValue("New request");
  await userEvent.keyboard("Track shipment{Enter}");
  expect(calls(ipc, "rename_request_item")).toEqual([
    { path: "requests/shipments/new-request.yaml", name: "Track shipment" },
  ]);
  await waitFor(() => expect(rowNames()).toContain("GETTrack shipment"));
  expect(row("Track shipment")).toHaveAttribute("aria-selected", "true");
  expect(tree).toHaveFocus();
});

test("new request goes into the selected folder", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await userEvent.click(row("Carriers"));
  await userEvent.click(screen.getByRole("button", { name: "New request" }));
  await screen.findByRole("textbox", { name: "Name" });
  expect(calls(ipc, "create_request")).toEqual([
    { folder: "requests/tracking/carriers", name: "New request" },
  ]);
});

test("new request in an empty project makes a collection first", async () => {
  const ipc = fakeRequestsIpc();
  render(CollectionsSidebar);
  await screen.findByText("No requests yet");
  await userEvent.click(screen.getByRole("button", { name: "New request" }));
  const input = await screen.findByRole("textbox", { name: "Name" });
  expect(calls(ipc, "create_folder")).toEqual([
    { parent: "requests", name: "New collection" },
  ]);
  expect(calls(ipc, "create_request")).toEqual([
    { folder: "requests/new-collection", name: "New request" },
  ]);
  expect(input).toHaveValue("New request");
});

test("New collection on the empty tree names it inline", async () => {
  const ipc = fakeRequestsIpc();
  render(CollectionsSidebar);
  await screen.findByText("No requests yet");
  await userEvent.click(screen.getByRole("button", { name: "New collection" }));
  const input = await screen.findByRole("textbox", { name: "Name" });
  expect(calls(ipc, "create_folder")).toEqual([
    { parent: "requests", name: "New collection" },
  ]);
  expect(input).toHaveValue("New collection");
  expect(input).toHaveFocus();
});

test("New folder from a folder's menu names it inline", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await openMenu("Tracking");
  await userEvent.click(screen.getByRole("menuitem", { name: "New folder" }));
  const input = await screen.findByRole("textbox", { name: "Name" });
  expect(calls(ipc, "create_folder")).toEqual([
    { parent: "requests/tracking", name: "New folder" },
  ]);
  expect(input).toHaveValue("New folder");
});

test("rename with Enter and cancel with Escape", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  const tree = await showTree();
  const tab = openTab(
    "requests.request",
    "Update status",
    "requests/shipments/update-status.yaml",
  );
  await userEvent.click(row("Update status"));
  tree.focus();
  await userEvent.keyboard("{F2}");
  const input = screen.getByRole("textbox", { name: "Name" });
  expect(input).toHaveFocus();
  await userEvent.clear(input);
  await userEvent.keyboard("Set status{Enter}");
  expect(calls(ipc, "rename_request_item")).toEqual([
    { path: "requests/shipments/update-status.yaml", name: "Set status" },
  ]);
  await waitFor(() => expect(rowNames()).toContain("PATCHSet status"));
  expect(tab).toMatchObject({
    title: "Set status",
    params: { path: "requests/shipments/set-status.yaml" },
  });
  expect(tree).toHaveFocus();

  // Escape leaves the name as it was.
  await userEvent.keyboard("{F2}");
  await userEvent.keyboard("{Backspace}{Backspace}{Escape}");
  expect(screen.queryByRole("textbox", { name: "Name" })).toBeNull();
  expect(rowNames()).toContain("PATCHSet status");
  expect(tree).toHaveFocus();

  // An empty name is refused and the field stays.
  await userEvent.keyboard("{F2}");
  await userEvent.clear(screen.getByRole("textbox", { name: "Name" }));
  await userEvent.keyboard("   {Enter}");
  const empty = screen.getByRole("textbox", { name: "Name" });
  expect(empty).toHaveAttribute("aria-invalid", "true");
  expect(calls(ipc, "rename_request_item")).toHaveLength(1);
});

test("renaming a folder updates open tabs below it", async () => {
  fakeRequestsIpc({ tree: shipments() });
  await showTree();
  const dhl = openTab(
    "requests.request",
    "DHL",
    "requests/tracking/carriers/dhl.yaml",
  );
  const events = openTab(
    "requests.request",
    "Tracking events",
    "requests/tracking/events.yaml",
  );
  const settings = openTab("requests.folder", "Tracking", "requests/tracking");
  const list = openTab(
    "requests.request",
    "List shipments",
    "requests/shipments/list.yaml",
  );
  await openMenu("Tracking");
  await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
  const input = screen.getByRole("textbox", { name: "Name" });
  await userEvent.clear(input);
  await userEvent.keyboard("Parcels{Enter}");
  await waitFor(() =>
    expect(dhl.params.path).toBe("requests/parcels/carriers/dhl.yaml"),
  );
  expect(dhl.title).toBe("DHL");
  expect(events.params.path).toBe("requests/parcels/events.yaml");
  expect(settings).toMatchObject({
    title: "Parcels",
    params: { path: "requests/parcels" },
  });
  expect(list.params.path).toBe("requests/shipments/list.yaml");
  // The folder stays open under its new name.
  await waitFor(() =>
    expect(row("Parcels")).toHaveAttribute("aria-expanded", "true"),
  );
});

test("duplicate calls the command and selects the copy", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await userEvent.click(row("List shipments"));
  await userEvent.click(screen.getByRole("button", { name: "More actions" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "Duplicate" }));
  expect(calls(ipc, "duplicate_request_item")).toEqual([
    { path: "requests/shipments/list.yaml" },
  ]);
  const copy = await screen.findByRole("treeitem", {
    name: named("List shipments copy"),
  });
  expect(copy).toHaveAttribute("aria-selected", "true");
});

test("a failed operation shows why and reloads the tree", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  ipc.failures.set("duplicate_request_item", {
    kind: "io",
    message: "the disk is full",
  });
  await showTree();
  const loads = () => calls(ipc, "request_tree").length;
  expect(loads()).toBe(1);
  await openMenu("List shipments");
  await userEvent.click(screen.getByRole("menuitem", { name: "Duplicate" }));
  await waitFor(() => expect(toasts.last.text).toBe("The disk is full"));
  await waitFor(() => expect(loads()).toBe(2));
});

test("a tree that cannot be read says why", async () => {
  const ipc = fakeRequestsIpc();
  ipc.failures.set("request_tree", {
    kind: "io",
    message: "the requests folder cannot be read",
  });
  render(CollectionsSidebar);
  await waitFor(() =>
    expect(toasts.last.text).toBe("The requests folder cannot be read"),
  );
});

test("move dialog moves with the keyboard", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  const tree = await showTree();
  tree.focus();
  await userEvent.keyboard("{ArrowDown}{ArrowDown}{Shift>}{F10}{/Shift}");
  expect(screen.getByRole("menu", { name: "List shipments" })).toBeVisible();
  await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");
  const dialog = screen.getByRole("dialog", { name: "Move List shipments" });
  const list = within(dialog).getByRole("listbox");
  expect(list).toHaveFocus();
  // Where it is now is not offered.
  expect(
    within(list)
      .getAllByRole("option")
      .map((o) => o.textContent),
  ).toEqual(["Tracking", "Carriers"]);
  expect(
    within(list).getByRole("option", { name: "Tracking" }),
  ).toHaveAttribute("aria-selected", "true");
  await userEvent.keyboard("{ArrowDown}");
  expect(
    within(list).getByRole("option", { name: "Carriers" }),
  ).toHaveAttribute("aria-selected", "true");
  await userEvent.keyboard("{Enter}");
  expect(calls(ipc, "move_request_item")).toEqual([
    {
      path: "requests/shipments/list.yaml",
      folder: "requests/tracking/carriers",
      index: 1,
    },
  ]);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  await waitFor(() => expect(tree).toHaveFocus());
  // It shows where it went.
  await waitFor(() =>
    expect(row("List shipments")).toHaveAttribute("aria-selected", "true"),
  );
  expect(row("Carriers")).toHaveAttribute("aria-expanded", "true");
});

test("the move dialog has Cancel and Move buttons", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await openMenu("Carriers");
  await userEvent.click(screen.getByRole("menuitem", { name: "Move to…" }));
  const dialog = screen.getByRole("dialog", { name: "Move Carriers" });
  // A folder cannot go into itself; Tracking holds it now.
  expect(
    within(dialog)
      .getAllByRole("option")
      .map((o) => o.textContent),
  ).toEqual(["Shipments"]);
  await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  await openMenu("Carriers");
  await userEvent.click(screen.getByRole("menuitem", { name: "Move to…" }));
  await userEvent.click(screen.getByRole("button", { name: "Move" }));
  expect(calls(ipc, "move_request_item")).toEqual([
    {
      path: "requests/tracking/carriers",
      folder: "requests/shipments",
      index: 3,
    },
  ]);
});

test("dragging a request onto another folder moves it to the end", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await drag(row("List shipments"), row("Tracking"), "middle");
  expect(ipc.calls).toContainEqual([
    "move_request_item",
    {
      path: "requests/shipments/list.yaml",
      folder: "requests/tracking",
      index: 2,
    },
  ]);
});

test("drag between rows sets the index", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  // Down within its folder: counted after it left its place.
  await drag(row("List shipments"), row("Update status"), "after");
  await waitFor(() =>
    expect(rowNames().slice(1, 4)).toEqual([
      "PATCHUpdate status",
      "GETList shipments",
      "POSTCreate shipment",
    ]),
  );
  // From another folder, between two rows.
  await drag(row("Tracking events"), row("Create shipment"), "before");
  await waitFor(() =>
    expect(rowNames().slice(1, 5)).toEqual([
      "PATCHUpdate status",
      "GETList shipments",
      "GETTracking events",
      "POSTCreate shipment",
    ]),
  );
  // Below an open folder's row is its first place.
  await drag(row("Update status"), row("Tracking"), "after");
  expect(calls(ipc, "move_request_item")).toEqual([
    {
      path: "requests/shipments/list.yaml",
      folder: "requests/shipments",
      index: 1,
    },
    {
      path: "requests/tracking/events.yaml",
      folder: "requests/shipments",
      index: 2,
    },
    {
      path: "requests/shipments/update-status.yaml",
      folder: "requests/tracking",
      index: 0,
    },
  ]);
});

test("only folders drop between collections", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await drag(row("List shipments"), row("Tracking"), "before");
  expect(calls(ipc, "move_request_item")).toEqual([]);
  await drag(row("Carriers"), row("Tracking"), "before");
  expect(calls(ipc, "move_request_item")).toEqual([
    { path: "requests/tracking/carriers", folder: "requests", index: 1 },
  ]);
});

test("a folder cannot be dropped into itself", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await showTree();
  await drag(row("Tracking"), row("Tracking events"), "before");
  await drag(row("Tracking"), row("Tracking"), "middle");
  expect(calls(ipc, "move_request_item")).toEqual([]);
});

test("escape cancels a drag", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  const { container } = render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await press(row("List shipments"));
  const y = await hover(row("Tracking"), "middle");
  expect(container.querySelector("[data-drop]")).not.toBeNull();
  await fireEvent.keyDown(window, { key: "Escape" });
  expect(container.querySelector("[data-drop]")).toBeNull();
  await fireEvent.pointerUp(row("Tracking"), pointer(y));
  expect(calls(ipc, "move_request_item")).toEqual([]);
});

test("a short press is a click, not a drag", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  const { container } = render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  const from = row("List shipments");
  const y = yOf(from, "middle");
  await fireEvent.pointerDown(from, pointer(y));
  await fireEvent.pointerMove(from, { ...pointer(y + 2), clientX: 62 });
  expect(container.querySelector("[data-drop]")).toBeNull();
  await fireEvent.pointerUp(from, { ...pointer(y + 2), clientX: 62 });
  expect(calls(ipc, "move_request_item")).toEqual([]);
});

test("a closed folder held under a drag opens after 600 ms", async () => {
  fakeRequestsIpc({ tree: shipments() });
  await showTree();
  vi.useFakeTimers();
  await press(row("List shipments"));
  await hover(row("Carriers"), "middle");
  vi.advanceTimersByTime(599);
  expect(row("Carriers")).toHaveAttribute("aria-expanded", "false");
  vi.advanceTimersByTime(1);
  await vi.waitFor(() =>
    expect(row("Carriers")).toHaveAttribute("aria-expanded", "true"),
  );
  await fireEvent.keyDown(window, { key: "Escape" });
});

test("the newRequest and newCollection commands show the sidebar", async () => {
  const ipc = fakeRequestsIpc({ tree: shipments() });
  await newRequest();
  expect(modules).toEqual(["requests"]);
  expect(calls(ipc, "create_request")).toEqual([
    { folder: "requests/shipments", name: "New request" },
  ]);
  expect(treeState.renaming).toBe("requests/shipments/new-request.yaml");
  await newCollection();
  expect(modules).toEqual(["requests", "requests"]);
  expect(calls(ipc, "create_folder")).toEqual([
    { parent: "requests", name: "New collection" },
  ]);
  expect(treeState.renaming).toBe("requests/new-collection");
});
