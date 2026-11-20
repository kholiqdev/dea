import { screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { openRequestTab, setUpRequestTabs } from "./request-tab-support.svelte";
import { resetTreeState } from "./state/tree.svelte";
import { fakeRequestsIpc, folder, leaf } from "./test-support";

// The code editor is a chunk of its own, which can fail to load.
vi.mock("@dea/editor/CodeEditor.svelte", () => {
  throw new Error("Failed to fetch dynamically imported module");
});

const UPLOAD = "requests/shipments/upload.yaml";

beforeEach(() => {
  resetTreeState();
  setUpRequestTabs();
});

test("a code editor that cannot load says so, and the tab still works", async () => {
  fakeRequestsIpc({
    tree: {
      collections: [
        folder("requests/shipments", "Shipments", [
          leaf(UPLOAD, "Upload", "POST", "https://harbor.test/upload"),
        ]),
      ],
      errors: [],
    },
    files: {
      [UPLOAD]: {
        name: "Upload",
        method: "POST",
        url: "https://harbor.test/upload",
        body: { mode: "json", text: "{}" },
      },
    },
  });
  openRequestTab(UPLOAD);
  await userEvent.click(await screen.findByRole("tab", { name: /Body/ }));
  expect(
    await screen.findByText("The editor could not load."),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("radio", { name: "Raw" }));
  expect(screen.getByRole("combobox", { name: "Raw language" })).toBeEnabled();
});
