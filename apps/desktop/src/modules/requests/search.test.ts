import { beforeEach, expect, test } from "vitest";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import requestItems from "./search";
import { resetTreeState, treeState } from "./state/tree.svelte";
import { fakeRequestsIpc, folder, leaf } from "./test-support";

let opened: [string, string, unknown][];
let activated: string[];
let toasts: string[];
let existing: Record<string, string>;

function shipments() {
  return {
    collections: [
      folder("requests/harbor", "Harbor", [
        folder("requests/harbor/ships", "Ships", [
          leaf(
            "requests/harbor/ships/shipment.yaml",
            "Shipment by ID",
            "GET",
            "{{baseUrl}}/v1/shipments/{{shipmentId}}",
          ),
        ]),
        leaf(
          "requests/harbor/health.yaml",
          "Health",
          "GET",
          "https://harbor.test/health",
        ),
      ]),
    ],
    errors: [],
  };
}

beforeEach(() => {
  resetTreeState();
  opened = [];
  activated = [];
  toasts = [];
  existing = {};
  setShellApi({
    openTab: (type, title, params) => {
      opened.push([type, title, params]);
      return "new";
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
    isTabDirty: () => false,
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    toast: (text) => {
      toasts.push(text);
      return 0;
    },
    dismissToast: () => {},
    runCommand: () => Promise.resolve(),
  } satisfies ShellApi);
});

test("requests appear in the palette with method and URL", async () => {
  fakeRequestsIpc({ tree: shipments() });
  const items = await requestItems();
  expect(items.map((i) => i.title)).toEqual(["Shipment by ID", "Health"]);
  expect(items.find((i) => i.title === "Shipment by ID")).toMatchObject({
    meta: "GET {{baseUrl}}/v1/shipments/{{shipmentId}}",
    opensTab: true,
  });
});

test("active target reuses the open tab and new target opens another", async () => {
  fakeRequestsIpc({ tree: shipments() });
  const [item] = await requestItems();
  existing["requests/harbor/ships/shipment.yaml"] = "tab-1";

  await item?.open("active");
  expect(activated).toEqual(["tab-1"]);
  expect(opened).toEqual([]);

  await item?.open("new");
  expect(opened).toEqual([
    [
      "requests.request",
      "Shipment by ID",
      { path: "requests/harbor/ships/shipment.yaml" },
    ],
  ]);
  expect(activated).toEqual(["tab-1"]);
});

test("active target opens a tab when none shows the request", async () => {
  fakeRequestsIpc({ tree: shipments() });
  const items = await requestItems();
  await items[1]?.open("active");
  expect(opened).toEqual([
    ["requests.request", "Health", { path: "requests/harbor/health.yaml" }],
  ]);
});

test("a removed request says so", async () => {
  fakeRequestsIpc({ tree: shipments() });
  const items = await requestItems();
  treeState.info = { collections: [], errors: [] };
  await items[1]?.open("active");
  expect(toasts).toEqual(["Health no longer exists"]);
  expect(opened).toEqual([]);
});
