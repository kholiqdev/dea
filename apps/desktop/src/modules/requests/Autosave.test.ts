import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import {
  openFolderTab,
  openRequestTab,
  setUpRequestTabs,
} from "./request-tab-support.svelte";
import { resetTreeState, treeState } from "./state/tree.svelte";
import {
  fakeRequestsIpc,
  folder,
  leaf,
  type RequestsIpc,
} from "./test-support";

const BY_ID = "requests/shipments/by-id.yaml";
const SHIPMENTS = "requests/shipments";

function tree(): RequestTreeInfo {
  return {
    collections: [folder(SHIPMENTS, "Shipments", [leaf(BY_ID, "By ID")])],
    errors: [],
  };
}

let setup: ReturnType<typeof setUpRequestTabs>;
let user: ReturnType<typeof userEvent.setup>;

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: false });
  user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  resetTreeState();
  setup = setUpRequestTabs();
  setAutosave(true);
});

afterEach(() => {
  setAutosave(false);
  vi.clearAllTimers();
  vi.useRealTimers();
});

function setAutosave(autosave: boolean): void {
  settings.apply({ ...DEFAULT_SETTINGS, requests: { autosave } });
}

async function setUpIpc(file: Partial<RequestFile> = {}): Promise<RequestsIpc> {
  const ipc = fakeRequestsIpc({
    tree: tree(),
    files: {
      [BY_ID]: {
        name: "By ID",
        method: "GET",
        url: "https://harbor.test/s",
        ...file,
      },
    },
  });
  await treeState.load();
  return ipc;
}

async function urlBox(): Promise<HTMLElement> {
  return await vi.waitFor(() => screen.getByRole("textbox", { name: "URL" }));
}

test("autosave writes a second after the last edit", async () => {
  const ipc = await setUpIpc();
  openRequestTab(BY_ID);
  await user.type(await urlBox(), "/x");
  await vi.advanceTimersByTimeAsync(999);
  expect(ipc.saved).toHaveLength(0);
  await vi.advanceTimersByTimeAsync(1);
  await waitFor(() => expect(ipc.saved).toHaveLength(1));
  expect(ipc.saved[0]?.url).toBe("https://harbor.test/s/x");
});

test("every edit starts the second again", async () => {
  const ipc = await setUpIpc();
  openRequestTab(BY_ID);
  const url = await urlBox();
  await user.type(url, "/a");
  await vi.advanceTimersByTimeAsync(800);
  await user.type(url, "b");
  await vi.advanceTimersByTimeAsync(800);
  expect(ipc.saved).toHaveLength(0);
  await vi.advanceTimersByTimeAsync(200);
  await waitFor(() => expect(ipc.saved).toHaveLength(1));
});

test("autosave is off by default", async () => {
  setAutosave(false);
  const ipc = await setUpIpc();
  openRequestTab(BY_ID);
  await user.type(await urlBox(), "/x");
  await vi.advanceTimersByTimeAsync(5000);
  expect(ipc.saved).toHaveLength(0);
});

test("a read-only project is not autosaved", async () => {
  const ipc = await setUpIpc();
  setup.project.info = { ...setup.project.info!, readOnly: true };
  openRequestTab(BY_ID);
  await urlBox();
  // The editor is locked, so the draft is changed from outside the view.
  const { drafts } = await import("./state/drafts.svelte");
  const draft = [...drafts.values()][0]!;
  draft.draft.url = "https://harbor.test/changed";
  await vi.advanceTimersByTimeAsync(5000);
  expect(ipc.saved).toHaveLength(0);
});

test("invalid_json_is_not_autosaved", async () => {
  const ipc = await setUpIpc({ body: { mode: "json", text: '{"a": 1}' } });
  openRequestTab(BY_ID);
  await urlBox();
  const { drafts } = await import("./state/drafts.svelte");
  const draft = [...drafts.values()][0]!;
  draft.setBody({ mode: "json", text: '{"a": ' });
  await vi.advanceTimersByTimeAsync(5000);
  expect(ipc.saved).toHaveLength(0);
  draft.setBody({ mode: "json", text: '{"a": 2}' });
  await vi.advanceTimersByTimeAsync(1000);
  await waitFor(() => expect(ipc.saved).toHaveLength(1));
});

test("autosave_waits_while_an_outside_change_is_open", async () => {
  const ipc = await setUpIpc();
  openRequestTab(BY_ID);
  await user.type(await urlBox(), "/x");
  const { drafts } = await import("./state/drafts.svelte");
  const draft = [...drafts.values()][0]!;
  draft.outside = "changed";
  await vi.advanceTimersByTimeAsync(5000);
  expect(ipc.saved).toHaveLength(0);
  draft.keepMine();
  await vi.advanceTimersByTimeAsync(1000);
  await waitFor(() => expect(ipc.saved).toHaveLength(1));
});

test("a closed tab is not autosaved", async () => {
  const ipc = await setUpIpc();
  const { id, unmount } = openRequestTab(BY_ID);
  await user.type(await urlBox(), "/x");
  unmount();
  // The tab is closed without a question, as when its edits are discarded.
  setup.shell.tabs = setup.shell.tabs.filter((t) => t.id !== id);
  const { pruneDrafts } = await import("./state/drafts.svelte");
  pruneDrafts();
  await vi.advanceTimersByTimeAsync(5000);
  expect(ipc.saved).toHaveLength(0);
});

test("a folder is autosaved too", async () => {
  const ipc = await setUpIpc();
  openFolderTab(SHIPMENTS);
  const { folderStates } = await import("./state/folders.svelte");
  await vi.waitFor(() =>
    expect([...folderStates.values()][0]?.saved).toBeTruthy(),
  );
  const state = [...folderStates.values()][0]!;
  state.setVariables([{ name: "host", value: "harbor.test" }]);
  await vi.advanceTimersByTimeAsync(999);
  expect(ipc.calls.filter(([c]) => c === "save_folder")).toHaveLength(0);
  await vi.advanceTimersByTimeAsync(1);
  await waitFor(() =>
    expect(ipc.calls.filter(([c]) => c === "save_folder")).toHaveLength(1),
  );
});

test("a half-set client certificate does not autosave", async () => {
  const ipc = await setUpIpc();
  openFolderTab(SHIPMENTS);
  const { folderStates } = await import("./state/folders.svelte");
  await vi.waitFor(() =>
    expect([...folderStates.values()][0]?.saved).toBeTruthy(),
  );
  const state = [...folderStates.values()][0]!;
  state.setTls({ clientCertificate: { certificate: "c.pem", key: "" } });
  await vi.advanceTimersByTimeAsync(5000);
  expect(ipc.calls.filter(([c]) => c === "save_folder")).toHaveLength(0);
});

test("a failed autosave toasts once and waits for the next edit", async () => {
  const ipc = await setUpIpc();
  ipc.failures.set("save_request", {
    kind: "readOnly",
    message: "this project is read-only",
  });
  openRequestTab(BY_ID);
  const url = await urlBox();
  const attempts = () => ipc.calls.filter(([c]) => c === "save_request");
  await user.type(url, "/x");
  await vi.advanceTimersByTimeAsync(1000);
  await waitFor(() => expect(setup.shell.toasts).toHaveLength(1));
  expect(attempts()).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(10000);
  expect(attempts()).toHaveLength(1);
  expect(setup.shell.toasts).toHaveLength(1);
  await user.type(url, "y");
  await vi.advanceTimersByTimeAsync(1000);
  await waitFor(() => expect(attempts()).toHaveLength(2));
  expect(setup.shell.toasts).toHaveLength(2);
});
