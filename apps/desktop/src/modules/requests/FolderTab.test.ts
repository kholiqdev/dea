import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { openFolderTab, setUpRequestTabs } from "./request-tab-support.svelte";
import { resetTreeState, treeState } from "./state/tree.svelte";
import {
  fakeRequestsIpc,
  folder,
  leaf,
  type RequestsIpc,
} from "./test-support";

const SHIPMENTS = "requests/shipments";
const ARCHIVE = "requests/shipments/archive";

function tree(): RequestTreeInfo {
  return {
    collections: [
      folder(SHIPMENTS, "Shipments", [
        leaf(`${SHIPMENTS}/by-id.yaml`, "By ID"),
        leaf(`${SHIPMENTS}/list.yaml`, "List"),
        folder(ARCHIVE, "Archive", [leaf(`${ARCHIVE}/old.yaml`, "Old")]),
      ]),
    ],
    errors: [],
  };
}

let shell: ReturnType<typeof setUpRequestTabs>["shell"];

beforeEach(() => {
  resetTreeState();
  ({ shell } = setUpRequestTabs());
});

async function setUp(secrets: string[] = []): Promise<RequestsIpc> {
  const ipc = fakeRequestsIpc({ tree: tree(), secrets });
  await treeState.load();
  return ipc;
}

test("folder tab text counts requests below", async () => {
  await setUp();
  openFolderTab(SHIPMENTS, "Shipments");
  expect(
    await screen.findByText(
      "These settings are inherited by the 3 requests in this collection. A request can override any of them.",
    ),
  ).toBeInTheDocument();
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
    "Shipments",
  );
});

test("a sub-folder says folder", async () => {
  await setUp();
  openFolderTab(ARCHIVE, "Archive");
  expect(
    await screen.findByText(/1 request in this folder/),
  ).toBeInTheDocument();
});

test("folder tab saves headers and variables", async () => {
  const ipc = await setUp();
  const { id } = openFolderTab(SHIPMENTS, "Shipments");
  await screen.findByRole("table", { name: "Headers" });
  await userEvent.click(screen.getByRole("button", { name: "Add header" }));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Header of row 1" }),
    "X-Team",
  );
  await userEvent.click(screen.getByRole("button", { name: "Add variable" }));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Name of row 1" }),
    "region",
  );
  expect(shell.tab(id).dirty).toBe(true);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(shell.tab(id).dirty).toBe(false));
  expect(ipc.folders.get(SHIPMENTS)).toMatchObject({
    headers: [{ name: "X-Team", value: "" }],
    variables: [{ name: "region", value: "" }],
  });
});

test("certificates only show for collections", async () => {
  await setUp();
  openFolderTab(SHIPMENTS, "Shipments");
  expect(await screen.findByText("Certificates")).toBeInTheDocument();
  openFolderTab(ARCHIVE, "Archive");
  await screen.findAllByText("Network");
  expect(screen.getAllByText("Certificates")).toHaveLength(1);
});

test("collection client key is chosen from Vault names", async () => {
  const ipc = fakeRequestsIpc({
    tree: tree(),
    secrets: ["client_key", "access_token"],
    pick: {
      value: "certs/client.pem",
      name: "client.pem",
      outside: false,
      missing: false,
    },
  });
  await treeState.load();
  const { id } = openFolderTab(SHIPMENTS, "Shipments");
  const select = await screen.findByRole("combobox", {
    name: "Key from Vault",
  });
  await waitFor(() =>
    expect(
      screen.getByRole("option", { name: "client_key" }),
    ).toBeInTheDocument(),
  );
  await userEvent.click(
    screen.getByRole("button", { name: /Client certificate/ }),
  );
  await userEvent.selectOptions(select, "client_key");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(shell.tab(id).dirty).toBe(false));
  expect(ipc.folders.get(SHIPMENTS)?.tls).toEqual({
    clientCertificate: {
      certificate: "certs/client.pem",
      key: "{{vault:client_key}}",
    },
  });
});

test("CA certificates are added to the list and saved", async () => {
  const ipc = fakeRequestsIpc({
    tree: tree(),
    pick: {
      value: "certs/ca.pem",
      name: "ca.pem",
      outside: false,
      missing: false,
    },
  });
  await treeState.load();
  const { id } = openFolderTab(SHIPMENTS, "Shipments");
  await userEvent.click(
    await screen.findByRole("button", { name: "Add CA certificate" }),
  );
  await userEvent.click(
    screen.getByRole("button", { name: "CA certificate 1" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(shell.tab(id).dirty).toBe(false));
  expect(ipc.folders.get(SHIPMENTS)?.tls).toEqual({
    caCertificates: ["certs/ca.pem"],
  });
});

test("a sub-folder shows the value its collection sets as inherited", async () => {
  const ipc = await setUp();
  ipc.folders.set(SHIPMENTS, { options: { timeoutMs: 8000 } });
  openFolderTab(ARCHIVE, "Archive");
  await waitFor(() =>
    expect(screen.getByRole("spinbutton", { name: "Timeout" })).toHaveValue(
      8000,
    ),
  );
});

test("a network option is saved and Reset takes it back out", async () => {
  const ipc = await setUp();
  const { id } = openFolderTab(SHIPMENTS, "Shipments");
  await userEvent.click(
    await screen.findByRole("switch", { name: "Verify TLS" }),
  );
  expect(
    screen.queryByRole("switch", { name: "Use the cookie jar" }),
  ).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(shell.tab(id).dirty).toBe(false));
  expect(ipc.folders.get(SHIPMENTS)?.options).toEqual({ verifyTls: false });
  await userEvent.click(
    screen.getByRole("button", { name: "Reset Verify TLS" }),
  );
  expect(shell.tab(id).dirty).toBe(true);
});
