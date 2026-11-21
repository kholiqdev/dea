import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import type { FakeProject } from "./fake-project.svelte";
import RequestTab from "./RequestTab.svelte";
import {
  openRequestTab,
  setUpRequestTabs,
  type FakeShell,
} from "./request-tab-support.svelte";
import { drafts } from "./state/drafts.svelte";
import { resetTreeState, treeState } from "./state/tree.svelte";
import {
  fakeRequestsIpc,
  folder,
  leaf,
  type RequestsIpc,
} from "./test-support";

const BY_ID = "requests/shipments/by-id.yaml";
const BASE = "{{baseUrl}}/v1/shipments/{{shipmentId}}";

function shipments(): RequestTreeInfo {
  return {
    collections: [
      folder("requests/shipments", "Shipments", [
        leaf(BY_ID, "Shipment by ID", "GET", BASE),
      ]),
      folder("requests/tracking", "Tracking", []),
    ],
    errors: [],
  };
}

function byId(): RequestFile {
  return { name: "Shipment by ID", method: "GET", url: BASE };
}

let shell: FakeShell;
let project: FakeProject;

beforeEach(() => {
  resetTreeState();
  ({ shell, project } = setUpRequestTabs());
  settings.apply(DEFAULT_SETTINGS);
});

const calls = (ipc: RequestsIpc, cmd: string) =>
  ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);

function shipmentsIpc(file: Partial<RequestFile> = {}): RequestsIpc {
  return fakeRequestsIpc({
    tree: shipments(),
    files: { [BY_ID]: { ...byId(), ...file } },
  });
}

const urlField = () =>
  screen.findByRole<HTMLInputElement>("textbox", { name: "URL" });

test("typing a query in the URL fills the table and Save writes both", async () => {
  const ipc = fakeRequestsIpc({
    tree: shipments(),
    files: { [BY_ID]: byId() },
  });
  openRequestTab(BY_ID);
  const url = await screen.findByRole("textbox", { name: "URL" });
  await userEvent.type(url, "?include=events");
  expect(screen.getByRole("checkbox", { name: "Send include" })).toBeChecked();
  expect(screen.getByRole("tab", { name: /Params 2/ })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  expect(ipc.saved.at(-1)).toMatchObject({
    url: "{{baseUrl}}/v1/shipments/{{shipmentId}}",
    params: [{ name: "include", value: "events", enabled: true }],
  });
});

test("the URL bar keeps what is typed while the table follows it", async () => {
  shipmentsIpc();
  openRequestTab(BY_ID);
  const url = await urlField();
  await userEvent.type(url, "?include&");
  expect(url).toHaveValue(`${BASE}?include&`);
  const table = screen.getByRole("table", { name: "Query parameters" });
  expect(
    within(table).getByRole("textbox", { name: "Key of row 1" }),
  ).toHaveValue("include");
});

test("editing_the_table_rewrites_the_url_bar", async () => {
  shipmentsIpc();
  openRequestTab(BY_ID);
  const url = await urlField();
  await userEvent.click(screen.getByRole("button", { name: "Add parameter" }));
  const key = screen.getByRole("textbox", { name: "Key of row 1" });
  expect(key).toHaveFocus();
  await userEvent.type(key, "limit");
  await userEvent.type(
    screen.getByRole("textbox", { name: "Value of limit" }),
    "20",
  );
  expect(url).toHaveValue(`${BASE}?limit=20`);
  // A row not filled in yet is not part of the URL.
  await userEvent.click(screen.getByRole("button", { name: "Add parameter" }));
  expect(url).toHaveValue(`${BASE}?limit=20`);
  await userEvent.click(screen.getByRole("checkbox", { name: "Send limit" }));
  expect(url).toHaveValue(BASE);
});

test("a row with only a description is not in the URL and not written", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID);
  const url = await urlField();
  await userEvent.click(screen.getByRole("button", { name: "Add parameter" }));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Description of row 1" }),
    "later",
  );
  expect(url).toHaveValue(BASE);
  expect(screen.getByRole("tab", { name: /Params 1/ })).toBeInTheDocument();
  await shell.save(id);
  expect(ipc.saved.at(-1)?.params).toBeUndefined();
});

test("moving_an_open_dirty_request_keeps_its_edits", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID, "Shipment by ID");
  const url = await urlField();
  await userEvent.type(url, "/events");
  await treeState.move(BY_ID, "requests/tracking", 0);

  const moved = "requests/tracking/by-id.yaml";
  expect(shell.tab(id).params.path).toBe(moved);
  expect(screen.getByRole("textbox", { name: "URL" })).toBe(url);
  expect(url).toHaveValue(`${BASE}/events`);
  expect(shell.tab(id).dirty).toBe(true);

  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  await waitFor(() =>
    expect(calls(ipc, "save_request")).toMatchObject([
      { path: moved, request: { url: `${BASE}/events` } },
    ]),
  );
  expect(ipc.files.has(BY_ID)).toBe(false);
  expect(shell.tab(id).dirty).toBe(false);
});

test("a sidebar rename keeps its name when the edited draft is saved", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID, "Shipment by ID");
  await userEvent.type(await urlField(), "/events");
  await treeState.rename(BY_ID, "Shipment details");

  const renamed = "requests/shipments/shipment-details.yaml";
  expect(shell.tab(id)).toMatchObject({
    title: "Shipment details",
    params: { path: renamed },
  });
  await shell.save(id);
  expect(calls(ipc, "save_request").at(-1)).toMatchObject({
    path: renamed,
    request: { name: "Shipment details", url: `${BASE}/events` },
  });
  expect(ipc.files.has(BY_ID)).toBe(false);
});

test("a renamed tab that is not shown saves to its new path", async () => {
  const ipc = shipmentsIpc();
  const { id, unmount } = openRequestTab(BY_ID, "Shipment by ID");
  await userEvent.type(await urlField(), "/events");
  // Another tab is shown while the sidebar renames this one.
  unmount();
  await treeState.rename(BY_ID, "Shipment details");

  // Closing it with Save, from the close dialog.
  await shell.save(id);
  const renamed = "requests/shipments/shipment-details.yaml";
  expect(calls(ipc, "save_request")).toMatchObject([
    {
      path: renamed,
      request: { name: "Shipment details", url: `${BASE}/events` },
    },
  ]);
  expect(ipc.files.has(BY_ID)).toBe(false);
});

test("outside_change_to_a_dirty_request_asks", async () => {
  const ipc = shipmentsIpc();
  openRequestTab(BY_ID);
  const url = await urlField();
  await userEvent.type(url, "/events");

  ipc.files.set(BY_ID, { ...byId(), method: "POST" });
  project.change([BY_ID]);
  await screen.findByText("This request changed on disk.");
  // The edits stay until the person decides.
  expect(url).toHaveValue(`${BASE}/events`);
  expect(screen.getByRole("button", { name: "Keep mine" })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Take theirs" }));
  await screen.findByRole("button", { name: "Method: POST" });
  expect(url).toHaveValue(BASE);
  expect(screen.queryByText("This request changed on disk.")).toBeNull();
});

test("keep mine keeps the edits and saves them over the disk", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  ipc.files.set(BY_ID, { ...byId(), method: "POST" });
  project.change([BY_ID]);

  await userEvent.click(
    await screen.findByRole("button", { name: "Keep mine" }),
  );
  expect(screen.queryByText("This request changed on disk.")).toBeNull();
  expect(shell.tab(id).dirty).toBe(true);
  await shell.save(id);
  expect(ipc.files.get(BY_ID)).toMatchObject({
    method: "GET",
    url: `${BASE}/events`,
  });
});

test("a tab that is not shown learns of a change on disk at once", async () => {
  const ipc = shipmentsIpc();
  const { id, unmount } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  unmount();

  ipc.files.set(BY_ID, { ...byId(), method: "POST" });
  project.change([BY_ID]);
  await waitFor(() => expect(drafts.get(id)?.outside).toBe("changed"));
});

test("Save from the close dialog does not write over a change on disk", async () => {
  const ipc = shipmentsIpc();
  const { id, unmount } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  unmount();
  ipc.files.set(BY_ID, { ...byId(), method: "POST" });
  project.change([BY_ID]);
  await waitFor(() => expect(drafts.get(id)?.outside).toBe("changed"));

  // The shell keeps a tab open when its save rejects.
  await expect(shell.save(id)).rejects.toThrow(
    "This request changed on disk. Choose Keep mine or Take theirs first.",
  );
  expect(calls(ipc, "save_request")).toEqual([]);
  expect(ipc.files.get(BY_ID)?.method).toBe("POST");
  // The tab is shown, so its banner can be answered.
  expect(shell.activated).toEqual([id]);

  render(RequestTab, { props: { tab: shell.tab(id) } });
  await userEvent.click(
    await screen.findByRole("button", { name: "Keep mine" }),
  );
  await shell.save(id);
  expect(ipc.files.get(BY_ID)).toMatchObject({
    method: "GET",
    url: `${BASE}/events`,
  });
});

test("a rename that keeps the file name is kept when a hidden tab saves", async () => {
  const path = "requests/shipments/get-user.yaml";
  const ipc = fakeRequestsIpc({
    tree: {
      collections: [
        folder("requests/shipments", "Shipments", [
          leaf(path, "get user", "GET", BASE),
        ]),
      ],
      errors: [],
    },
    files: { [path]: { name: "get user", method: "GET", url: BASE } },
  });
  const { id, unmount } = openRequestTab(path, "get user");
  await userEvent.type(await urlField(), "/events");
  unmount();

  await treeState.rename(path, "Get user");
  expect(shell.tab(id).params.path).toBe(path);
  await shell.save(id);
  expect(calls(ipc, "save_request").at(-1)).toMatchObject({
    path,
    request: { name: "Get user", url: `${BASE}/events` },
  });
});

test("clean_draft_reloads_silently_on_outside_change", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID, "Shipment by ID");
  const url = await urlField();

  const v2 = "{{baseUrl}}/v2/shipments/{{shipmentId}}";
  ipc.files.set(BY_ID, { ...byId(), name: "Shipment v2", url: v2 });
  project.change([BY_ID]);
  await waitFor(() => expect(url).toHaveValue(v2));
  expect(screen.queryByText("This request changed on disk.")).toBeNull();
  expect(shell.tab(id)).toMatchObject({ title: "Shipment v2", dirty: false });
});

test("a save reported back by the watcher raises nothing", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "?include=events");
  await shell.save(id);

  project.change([BY_ID]);
  await waitFor(() => expect(calls(ipc, "read_request")).toHaveLength(2));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByText("This request changed on disk.")).toBeNull();
  expect(shell.tab(id).dirty).toBe(false);
});

test("deleted_on_disk_offers_save_again", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");

  // Deleting from the sidebar leaves a tab with edits open.
  await treeState.remove(BY_ID);
  expect(shell.tab(id)).toBeTruthy();
  project.change([BY_ID]);
  await screen.findByText("This request was deleted on disk.");
  expect(screen.getByRole("button", { name: "Close tab" })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Save again" }));
  await waitFor(() =>
    expect(ipc.files.get(BY_ID)).toMatchObject({ url: `${BASE}/events` }),
  );
  expect(screen.queryByText("This request was deleted on disk.")).toBeNull();
});

test("close tab on a deleted request asks the shell to close it", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  ipc.files.delete(BY_ID);
  ipc.tree = { collections: [], errors: [] };
  project.change([BY_ID]);

  await userEvent.click(
    await screen.findByRole("button", { name: "Close tab" }),
  );
  expect(shell.closing).toEqual([id]);
});

test("a tab without edits whose file is deleted on disk says it is gone", async () => {
  const ipc = shipmentsIpc();
  openRequestTab(BY_ID);
  await urlField();
  ipc.files.delete(BY_ID);
  ipc.tree = { collections: [], errors: [] };
  project.change([BY_ID]);
  expect(
    await screen.findByRole("heading", {
      name: "This request no longer exists",
    }),
  ).toBeInTheDocument();
  expect(screen.queryByText("This request was deleted on disk.")).toBeNull();
});

test("a restored tab whose request is gone says so", async () => {
  fakeRequestsIpc({ tree: shipments() });
  openRequestTab("requests/shipments/gone.yaml");
  expect(
    await screen.findByRole("heading", {
      name: "This request no longer exists",
    }),
  ).toBeInTheDocument();
});

test("custom_method_is_accepted_and_shown", async () => {
  const ipc = shipmentsIpc();
  const { id } = openRequestTab(BY_ID);
  await userEvent.click(
    await screen.findByRole("button", { name: "Method: GET" }),
  );
  const menu = screen.getByRole("menu");
  expect(
    within(menu)
      .getAllByRole("menuitemradio")
      .map((item) => item.textContent?.trim()),
  ).toEqual([
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
    "Custom…",
  ]);
  await userEvent.click(
    within(menu).getByRole("menuitemradio", { name: "Custom…" }),
  );

  const input = screen.getByRole("textbox", { name: "Custom method" });
  expect(input).toHaveFocus();
  // Not an HTTP token: Enter does nothing.
  await userEvent.type(input, "NOT OK{Enter}");
  expect(input).toHaveAttribute("aria-invalid", "true");
  await userEvent.clear(input);
  await userEvent.type(input, "PURGE{Enter}");
  expect(
    screen.getByRole("button", { name: "Method: PURGE" }),
  ).toBeInTheDocument();

  await shell.save(id);
  expect(ipc.saved.at(-1)?.method).toBe("PURGE");
});

test("leaving the custom method field keeps a valid method and drops an invalid one", async () => {
  shipmentsIpc();
  openRequestTab(BY_ID);
  const custom = async () => {
    await userEvent.click(
      await screen.findByRole("button", { name: /^Method: / }),
    );
    await userEvent.click(
      screen.getByRole("menuitemradio", { name: "Custom…" }),
    );
    return screen.getByRole("textbox", { name: "Custom method" });
  };
  await userEvent.type(await custom(), "PURGE");
  await userEvent.click(await urlField());
  expect(
    screen.getByRole("button", { name: "Method: PURGE" }),
  ).toBeInTheDocument();

  const input = await custom();
  await userEvent.clear(input);
  await userEvent.type(input, "NOT OK");
  await userEvent.click(await urlField());
  expect(
    screen.getByRole("button", { name: "Method: PURGE" }),
  ).toBeInTheDocument();
});

test("a standard method is chosen from the menu", async () => {
  shipmentsIpc();
  openRequestTab(BY_ID);
  await userEvent.click(
    await screen.findByRole("button", { name: "Method: GET" }),
  );
  await userEvent.click(screen.getByRole("menuitemradio", { name: "PATCH" }));
  expect(
    screen.getByRole("button", { name: "Method: PATCH" }),
  ).toBeInTheDocument();
});

test("path_variables_follow_the_url", async () => {
  const ipc = shipmentsIpc({
    url: "https://harbor.test/shipments/:id",
    pathVariables: [{ name: "id", value: "shp_1", description: "Shipment ID" }],
  });
  const { id } = openRequestTab(BY_ID);
  const url = await urlField();
  const table = screen.getByRole("table", { name: "Path variables" });
  expect(within(table).getByText("id")).toBeInTheDocument();
  expect(
    within(table).getByRole("textbox", { name: "Value of id" }),
  ).toHaveValue("shp_1");
  // Names follow the URL, so they cannot be typed over.
  expect(
    within(table).queryByRole("textbox", { name: "Key of row 1" }),
  ).toBeNull();

  await userEvent.type(url, "/events/:eventId");
  await userEvent.type(
    within(table).getByRole("textbox", { name: "Value of eventId" }),
    "evt_9",
  );
  expect(screen.getByRole("tab", { name: /Params 2/ })).toBeInTheDocument();
  await shell.save(id);
  expect(ipc.saved.at(-1)?.pathVariables).toEqual([
    { name: "id", value: "shp_1", description: "Shipment ID" },
    { name: "eventId", value: "evt_9" },
  ]);

  // Without path variables the table goes; it comes back with the new ones.
  await userEvent.clear(url);
  expect(screen.queryByRole("table", { name: "Path variables" })).toBeNull();
  await userEvent.type(url, "https://harbor.test/events/:eventId");
  const after = screen.getByRole("table", { name: "Path variables" });
  expect(within(after).queryByText("id")).toBeNull();
  expect(within(after).getByText("eventId")).toBeInTheDocument();
});

test("automatic_headers_show_and_replaced_ones_are_marked", async () => {
  fakeRequestsIpc({
    tree: shipments(),
    files: { [BY_ID]: byId() },
    inspection: {
      automaticHeaders: [
        { name: "User-Agent", value: "Dea/0.1.0", replaced: false },
        { name: "Accept", value: "*/*", replaced: true },
        { name: "Content-Length", value: null, replaced: false },
      ],
    },
  });
  openRequestTab(BY_ID);
  await userEvent.click(await screen.findByRole("tab", { name: /Headers/ }));
  expect(
    screen.getByText(
      "Added when the request is sent. A header with the same name above replaces it.",
    ),
  ).toBeInTheDocument();
  const list = await screen.findByRole("list", { name: "Automatic headers" });
  const [agent, accept, length] = within(list).getAllByRole("listitem");
  expect(agent).toHaveTextContent("User-AgentDea/0.1.0");
  expect(agent).not.toHaveTextContent("Replaced");
  expect(accept).toHaveTextContent("Replaced");
  expect(accept?.querySelector("s")).toHaveTextContent("Accept");
  expect(length).toHaveTextContent("Content-LengthCalculated when sent");
});

test("header_count_counts_enabled_rows_only", async () => {
  shipmentsIpc({
    headers: [
      { name: "Accept", value: "application/json" },
      { name: "X-Trace", value: "1", enabled: false },
      { name: "X-Carrier", value: "dhl" },
    ],
  });
  openRequestTab(BY_ID);
  await userEvent.click(await screen.findByRole("tab", { name: "Headers 2" }));
  await userEvent.click(screen.getByRole("checkbox", { name: "Send Accept" }));
  expect(screen.getByRole("tab", { name: "Headers 1" })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("checkbox", { name: "Send X-Trace" }));
  expect(screen.getByRole("tab", { name: "Headers 2" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Settings" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Body" })).toBeInTheDocument();
});

test("undefined_variables_in_headers_are_red", async () => {
  fakeRequestsIpc({
    tree: shipments(),
    files: {
      [BY_ID]: {
        ...byId(),
        headers: [{ name: "X-Carrier", value: "{{carrierCode}}" }],
      },
    },
    inspection: { undefined: ["carrierCode"] },
  });
  openRequestTab(BY_ID);
  await userEvent.click(await screen.findByRole("tab", { name: /Headers/ }));
  const table = screen.getByRole("table", { name: "Headers" });
  await waitFor(() =>
    expect(table.querySelector(".undefined-var")).toHaveTextContent(
      "{{carrierCode}}",
    ),
  );
});

test("read_only_project_disables_the_editor", async () => {
  project.refresh({ ...project.info!, readOnly: true });
  shipmentsIpc({ headers: [{ name: "Accept", value: "application/json" }] });
  openRequestTab(BY_ID);
  expect(await urlField()).toBeDisabled();
  expect(screen.getByRole("button", { name: "Method: GET" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Save request" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Add parameter" })).toBeDisabled();

  await userEvent.click(screen.getByRole("tab", { name: /Headers/ }));
  expect(screen.getByRole("checkbox", { name: "Send Accept" })).toBeDisabled();
  expect(screen.getByRole("textbox", { name: "Key of row 1" })).toBeDisabled();
  expect(
    screen.getByRole("textbox", { name: "Value of Accept" }),
  ).toBeDisabled();
  expect(screen.getByRole("button", { name: "Add header" })).toBeDisabled();
});

test("a failed inspection shows why", async () => {
  const ipc = shipmentsIpc();
  ipc.failures.set("inspect_request", {
    kind: "invalid",
    message:
      "requests/shipments/_folder.yaml is not valid: unknown field `colour`",
  });
  openRequestTab(BY_ID);
  expect(
    await screen.findByText(
      "Requests/shipments/_folder.yaml is not valid: unknown field `colour`",
    ),
  ).toBeInTheDocument();
});

test("inspection waits for the last change, and runs again for a new environment", async () => {
  const ipc = shipmentsIpc();
  openRequestTab(BY_ID);
  await waitFor(() => expect(calls(ipc, "inspect_request")).toHaveLength(1));
  await userEvent.type(await urlField(), "/events");
  await waitFor(() => expect(calls(ipc, "inspect_request")).toHaveLength(2));
  expect(calls(ipc, "inspect_request").at(-1)).toMatchObject({
    path: BY_ID,
    request: { url: `${BASE}/events` },
  });
  await new Promise((resolve) => setTimeout(resolve, 250));
  expect(calls(ipc, "inspect_request")).toHaveLength(2);

  await project.setEnvironment("staging");
  await waitFor(() => expect(calls(ipc, "inspect_request")).toHaveLength(3));
});

test("an older inspection never replaces a newer one", async () => {
  let release = () => {};
  const slow = new Promise<void>((resolve) => (release = resolve));
  fakeRequestsIpc({
    tree: shipments(),
    files: { [BY_ID]: byId() },
    inspection: async (_path, request) => {
      if (request.url.endsWith("/v2"))
        return {
          automaticHeaders: [{ name: "X-New", value: "2", replaced: false }],
        };
      await slow;
      return {
        automaticHeaders: [{ name: "X-Old", value: "1", replaced: false }],
      };
    },
  });
  openRequestTab(BY_ID);
  await userEvent.click(await screen.findByRole("tab", { name: /Headers/ }));
  // The first inspection is still out when the URL changes.
  await new Promise((resolve) => setTimeout(resolve, 250));
  await userEvent.type(await urlField(), "/v2");
  await screen.findByText("X-New");
  release();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByText("X-Old")).toBeNull();
  expect(screen.getByText("X-New")).toBeInTheDocument();
});

test("switching to another tab and back keeps the edits", async () => {
  shipmentsIpc();
  const { id, unmount } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  unmount();

  render(RequestTab, { props: { tab: shell.tab(id) } });
  expect(await urlField()).toHaveValue(`${BASE}/events`);
  expect(shell.tab(id).dirty).toBe(true);
});

test("a tab closed while not shown forgets its edits when reopened", async () => {
  shipmentsIpc();
  const { id, unmount } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  unmount();
  // Closed from the tab bar with Discard, then reopened with its id.
  const closed = shell.tab(id);
  shell.tabs = shell.tabs.filter((t) => t.id !== id);
  shell.tabs.push({ ...closed, dirty: false });

  render(RequestTab, { props: { tab: shell.tab(id) } });
  expect(await urlField()).toHaveValue(BASE);
  expect(shell.tab(id).dirty).toBe(false);
});

test("the draft of a tab closed while not shown is dropped", async () => {
  fakeRequestsIpc({
    tree: shipments(),
    files: { [BY_ID]: byId(), "requests/shipments/list.yaml": byId() },
  });
  const first = openRequestTab(BY_ID);
  await urlField();
  first.unmount();
  shell.tabs = shell.tabs.filter((t) => t.id !== first.id);
  expect(drafts.has(first.id)).toBe(true);

  openRequestTab("requests/shipments/list.yaml");
  await urlField();
  expect(drafts.has(first.id)).toBe(false);
});

test("a closed tab forgets its edits, even if it is reopened", async () => {
  shipmentsIpc();
  const { id, unmount } = openRequestTab(BY_ID);
  await userEvent.type(await urlField(), "/events");
  // Closing with Discard: the shell drops the tab, then its view.
  const closed = shell.tab(id);
  shell.tabs = shell.tabs.filter((t) => t.id !== id);
  unmount();

  // Reopening the closed tab brings back its id.
  shell.tabs.push({ ...closed, dirty: false });
  render(RequestTab, { props: { tab: shell.tab(id) } });
  expect(await urlField()).toHaveValue(BASE);
  expect(shell.tab(id).dirty).toBe(false);
});

test("changing_the_method_updates_the_tab", async () => {
  shipmentsIpc();
  const { id } = openRequestTab({ path: BY_ID, method: "GET" });
  await userEvent.click(
    await screen.findByRole("button", { name: "Method: GET" }),
  );
  await userEvent.click(
    within(screen.getByRole("menu")).getByRole("menuitemradio", {
      name: "DELETE",
    }),
  );
  await waitFor(() =>
    expect(shell.tab(id).params).toEqual({ path: BY_ID, method: "DELETE" }),
  );
});

test("the_final_url_shows_under_the_bar", async () => {
  const shown = "https://api.harbor.test/v1/shipments/shp_4821";
  const ipc = fakeRequestsIpc({
    tree: shipments(),
    files: { [BY_ID]: byId() },
    inspection: { url: shown },
  });
  openRequestTab(BY_ID);
  const url = await screen.findByRole("button", {
    name: "Copy the final URL",
  });
  expect(url).toHaveTextContent(shown);
  await userEvent.click(url);
  expect(ipc.copiedTexts).toEqual([shown]);
  await waitFor(() => expect(shell.toasts).toContain("URL copied"));
});

test("the_request_bar_spans_the_tab", async () => {
  shipmentsIpc();
  openRequestTab(BY_ID);
  const save = await screen.findByRole("button", { name: "Save request" });
  const send = screen.getByRole("button", { name: "Send" });
  expect(send).toHaveAttribute("data-send");
  expect(document.querySelector(".split")).toBeInTheDocument();
  expect(save.closest(".split")).toBeNull();
  expect(send.closest(".split")).toBeNull();
  // The final URL comes with the first inspection.
  const url = await screen.findByRole("button", { name: "Copy the final URL" });
  expect(url.closest(".split")).toBeNull();
});

const divider = () =>
  screen.findByRole("separator", { name: "Resize the request and response" });

test("a_resized_split_is_saved", async () => {
  shipmentsIpc();
  const update = vi.spyOn(settings, "update");
  try {
    openRequestTab(BY_ID);
    const handle = await divider();
    handle.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(update).toHaveBeenCalledWith({
      requests: { responseSplit: { right: 46, below: 40 } },
    });
    await waitFor(() =>
      expect(settings.requests.responseSplit).toEqual({ right: 46, below: 40 }),
    );
    expect(handle).toHaveAttribute("aria-valuenow", "46");
  } finally {
    update.mockRestore();
  }
});

test("a_refused_split_save_says_why_and_puts_the_divider_back", async () => {
  const ipc = shipmentsIpc();
  ipc.failures.set("save_settings", {
    kind: "io",
    message: "the settings file is read-only",
  });
  openRequestTab(BY_ID);
  const handle = await divider();
  handle.focus();
  await userEvent.keyboard("{ArrowRight}");
  await waitFor(() =>
    expect(shell.toasts).toContain("The settings file is read-only"),
  );
  expect(settings.requests.responseSplit).toEqual({ right: 44, below: 40 });
  expect(handle).toHaveAttribute("aria-valuenow", "44");
});

test("an_older_tab_learns_its_method_when_the_draft_loads", async () => {
  shipmentsIpc({ method: "PUT" });
  const { id } = openRequestTab(BY_ID);
  await waitFor(() =>
    expect(shell.tab(id).params).toEqual({ path: BY_ID, method: "PUT" }),
  );
});
