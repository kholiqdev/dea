import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import { FakeProject } from "./fake-project.svelte";
import CollectionsSidebar from "./components/CollectionsSidebar.svelte";
import { resetTreeState } from "./state/tree.svelte";
import { bigTree, fakeRequestsIpc, folder, leaf } from "./test-support";

let project: FakeProject;
let opened: [string, string, unknown][];
let activated: string[];
let existing: Record<string, string>;

function harborTree() {
  return {
    collections: [
      folder("requests/harbor", "Harbor", [
        folder("requests/harbor/ships", "Ships", [
          leaf(
            "requests/harbor/ships/list.yaml",
            "List ships",
            "GET",
            "https://harbor.test/ships",
          ),
          leaf(
            "requests/harbor/ships/dock.yaml",
            "Dock a ship",
            "POST",
            "https://harbor.test/berths",
          ),
        ]),
        leaf(
          "requests/harbor/health.yaml",
          "Health",
          "GET",
          "https://harbor.test/health",
        ),
      ]),
      folder("requests/cargo", "Cargo", [
        leaf(
          "requests/cargo/manifest.yaml",
          "Manifest",
          "GET",
          "https://cargo.test/m",
        ),
      ]),
    ],
    errors: [],
  };
}

beforeEach(() => {
  performance.clearMarks();
  performance.clearMeasures();
  resetTreeState();
  opened = [];
  activated = [];
  existing = {};
  project = new FakeProject([
    { name: "local", production: false },
    { name: "production", production: true },
  ]);
  setProjectApi(project);
  setShellApi({
    openTab: (type, title, params) => {
      opened.push([type, title, params]);
      return "t";
    },
    findTab: (_type, match) => {
      for (const [path, id] of Object.entries(existing))
        if (match?.({ path })) return id;
      return undefined;
    },
    updateTab: () => {},
    activateTab: (id) => {
      activated.push(id);
      return true;
    },
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    toast: () => 0,
    dismissToast: () => {},
    runCommand: () => Promise.resolve(),
  } satisfies ShellApi);
});

const filterBox = () =>
  screen.getByRole("searchbox", { name: "Filter collections" });
const rowNames = () =>
  screen.getAllByRole("treeitem").map((r) => r.textContent);

test("5,000 requests filter within 100 ms", async () => {
  fakeRequestsIpc({ tree: bigTree(5000) });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.type(filterBox(), "shipment 4999");
  await waitFor(() =>
    expect(
      performance.getEntriesByName("requests-filter").length,
    ).toBeGreaterThan(0),
  );
  // Every keystroke counts; the first ones match all 5,000 requests.
  const entries = performance.getEntriesByName("requests-filter");
  expect(entries.length).toBe("shipment 4999".length);
  expect(Math.max(...entries.map((e) => e.duration))).toBeLessThan(100);
  expect(screen.getAllByRole("treeitem").map((r) => r.textContent)).toContain(
    "GETshipment 4999",
  );
});

test("filter matches urls and opens parent folders", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.type(filterBox(), "BERTHS");
  expect(rowNames()).toEqual(["Harbor3", "Ships2", "POSTDock a ship"]);
  const ships = screen.getByRole("treeitem", { name: /Ships/ });
  expect(ships).toHaveAttribute("aria-expanded", "true");
  expect(ships).toHaveAttribute("aria-level", "2");
});

test("a folder whose name matches shows everything inside it", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.type(filterBox(), "cargo");
  expect(rowNames()).toEqual(["Cargo1", "GETManifest"]);
});

test("arrow keys move and expand", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  const tree = await screen.findByRole("tree", { name: "Collections" });
  tree.focus();
  const user = userEvent.setup();
  expect(rowNames()).toEqual(["Harbor3", "Cargo1"]);
  await user.keyboard("{ArrowDown}");
  expect(screen.getByRole("treeitem", { name: /Harbor/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await user.keyboard("{ArrowRight}");
  expect(rowNames()).toEqual(["Harbor3", "Ships2", "GETHealth", "Cargo1"]);
  await user.keyboard("{ArrowRight}{ArrowRight}{ArrowRight}");
  expect(screen.getByRole("treeitem", { name: /List ships/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await user.keyboard("{ArrowLeft}");
  expect(screen.getByRole("treeitem", { name: /Ships/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await user.keyboard("{ArrowLeft}");
  expect(screen.getByRole("treeitem", { name: /Ships/ })).toHaveAttribute(
    "aria-expanded",
    "false",
  );
  await user.keyboard("{End}");
  expect(screen.getByRole("treeitem", { name: /Cargo/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await user.keyboard("{Home}");
  expect(screen.getByRole("treeitem", { name: /Harbor/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await user.keyboard("{ArrowLeft}");
  expect(screen.getByRole("treeitem", { name: /Harbor/ })).toHaveAttribute(
    "aria-expanded",
    "false",
  );
});

test("Enter opens the selected request", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  const tree = await screen.findByRole("tree", { name: "Collections" });
  await userEvent.click(screen.getByRole("treeitem", { name: /Cargo/ }));
  tree.focus();
  await userEvent.keyboard("{ArrowDown}{Enter}");
  expect(opened).toEqual([
    ["requests.request", "Manifest", { path: "requests/cargo/manifest.yaml" }],
  ]);
});

test("clicking a request opens or activates its tab", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.click(screen.getByRole("treeitem", { name: /Cargo/ }));
  await userEvent.click(screen.getByRole("treeitem", { name: /Manifest/ }));
  expect(opened).toEqual([
    ["requests.request", "Manifest", { path: "requests/cargo/manifest.yaml" }],
  ]);
  existing["requests/cargo/manifest.yaml"] = "tab-7";
  await userEvent.click(screen.getByRole("treeitem", { name: /Manifest/ }));
  expect(opened).toHaveLength(1);
  expect(activated).toEqual(["tab-7"]);
});

test("changes under requests reload the tree", async () => {
  const ipc = fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  const loads = () => ipc.calls.filter(([c]) => c === "request_tree").length;
  expect(loads()).toBe(1);
  ipc.tree.collections.push(folder("requests/new", "Fresh", []));
  project.change(["environments/local.yaml"]);
  await new Promise((r) => setTimeout(r, 20));
  expect(loads()).toBe(1);
  project.change(["requests/new/_folder.yaml"]);
  await screen.findByRole("treeitem", { name: /Fresh/ });
  expect(loads()).toBe(2);
});

test("only visible rows are rendered", async () => {
  fakeRequestsIpc({ tree: bigTree(5000) });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.click(screen.getByRole("treeitem", { name: /Shipments/ }));
  await waitFor(() =>
    expect(screen.getAllByRole("treeitem").length).toBeGreaterThan(10),
  );
  expect(screen.getAllByRole("treeitem").length).toBeLessThanOrEqual(60);
});

test("environment rows set the active environment", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  const section = screen.getByRole("heading", {
    name: "Environments",
  }).parentElement!;
  expect(within(section).getByText("production")).toHaveClass("env-prod");
  expect(within(section).getByText("local")).not.toHaveClass("env-prod");
  await userEvent.click(within(section).getByRole("button", { name: /local/ }));
  expect(project.chosen).toEqual(["local"]);
});

test("a broken folder shows a warning", async () => {
  fakeRequestsIpc({
    tree: {
      collections: [folder("requests/odd", "Odd", [], true)],
      errors: [],
    },
  });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  expect(screen.getByTitle("_folder.yaml has a problem")).toBeInTheDocument();
});

test("an empty tree offers a first collection", async () => {
  fakeRequestsIpc();
  render(CollectionsSidebar);
  expect(await screen.findByText("No requests yet")).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "New collection" }),
  ).toBeInTheDocument();
});

test("/ focuses the filter unless a text field has the focus", async () => {
  fakeRequestsIpc({ tree: harborTree() });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.keyboard("/");
  expect(filterBox()).toHaveFocus();
  expect(filterBox()).toHaveValue("");
  await userEvent.keyboard("a/");
  expect(filterBox()).toHaveValue("a/");
});

test("rows tell their place among their siblings", async () => {
  fakeRequestsIpc({
    tree: {
      collections: [
        folder("requests/a", "A", [
          leaf("requests/a/1.yaml", "one"),
          leaf("requests/a/2.yaml", "two"),
          leaf("requests/a/3.yaml", "three"),
          leaf("requests/a/4.yaml", "four"),
        ]),
        folder("requests/b", "B", [leaf("requests/b/1.yaml", "solo")]),
      ],
      errors: [],
    },
  });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.click(screen.getByRole("treeitem", { name: /^A/ }));
  await userEvent.click(screen.getByRole("treeitem", { name: /^B/ }));
  const three = screen.getByRole("treeitem", { name: /three/ });
  expect(three).toHaveAttribute("aria-posinset", "3");
  expect(three).toHaveAttribute("aria-setsize", "4");
  const solo = screen.getByRole("treeitem", { name: /solo/ });
  expect(solo).toHaveAttribute("aria-posinset", "1");
  expect(solo).toHaveAttribute("aria-setsize", "1");
  expect(screen.getByRole("treeitem", { name: /^B/ })).toHaveAttribute(
    "aria-posinset",
    "2",
  );
});

test("a row far down a long list keeps its place when scrolled to", async () => {
  fakeRequestsIpc({ tree: bigTree(5000) });
  render(CollectionsSidebar);
  await screen.findByRole("tree", { name: "Collections" });
  await userEvent.click(screen.getByRole("treeitem", { name: /Shipments/ }));
  await userEvent.keyboard("{End}");
  const last = await screen.findByRole("treeitem", { name: /shipment 4999/ });
  expect(last).toHaveAttribute("aria-posinset", "5000");
  expect(last).toHaveAttribute("aria-setsize", "5000");
});
