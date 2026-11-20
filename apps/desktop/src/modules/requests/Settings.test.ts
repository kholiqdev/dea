import { screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { RequestTreeInfo } from "$lib/ipc/bindings/RequestTreeInfo";
import { openRequestTab, setUpRequestTabs } from "./request-tab-support.svelte";
import { resetTreeState } from "./state/tree.svelte";
import {
  fakeRequestsIpc,
  folder,
  leaf,
  type RequestsIpc,
} from "./test-support";

const BY_ID = "requests/shipments/by-id.yaml";

function tree(): RequestTreeInfo {
  return {
    collections: [
      folder("requests/shipments", "Shipments", [
        leaf(BY_ID, "Shipment by ID", "GET", "https://harbor.test/s"),
      ]),
    ],
    errors: [],
  };
}

const DEFAULTS = {
  timeoutMs: 30000,
  followRedirects: true,
  maxRedirects: 10,
  verifyTls: true,
  useSystemProxy: true,
  cookies: true,
};

beforeEach(() => {
  resetTreeState();
  setUpRequestTabs();
});

function setUp(
  file: Partial<RequestFile> = {},
  inherited: Partial<typeof DEFAULTS> = {},
): RequestsIpc {
  return fakeRequestsIpc({
    tree: tree(),
    files: {
      [BY_ID]: {
        name: "Shipment by ID",
        method: "GET",
        url: "https://harbor.test/s",
        ...file,
      },
    },
    // The inspection answers what the draft would use.
    inspection: (_path, request) => ({
      options: { ...DEFAULTS, ...inherited, ...request.options },
    }),
  });
}

async function openSettings(): Promise<void> {
  openRequestTab(BY_ID);
  await userEvent.click(await screen.findByRole("tab", { name: "Settings" }));
}

const row = (name: string) => within(screen.getByRole("group", { name }));

test("an inherited timeout shows the collection value and Reset clears an override", async () => {
  setUp({}, { timeoutMs: 8000 });
  await openSettings();
  await waitFor(() =>
    expect(screen.getByRole("spinbutton", { name: "Timeout" })).toHaveValue(
      8000,
    ),
  );
  expect(row("Timeout").getByText("Inherited")).toBeInTheDocument();
  expect(row("Timeout").queryByRole("button", { name: /Reset/ })).toBeNull();
});

test("reset returns a row to inherited", async () => {
  const ipc = setUp({ options: { timeoutMs: 500 } }, {});
  await openSettings();
  expect(screen.getByRole("spinbutton", { name: "Timeout" })).toHaveValue(500);
  await userEvent.click(row("Timeout").getByRole("button", { name: /Reset/ }));
  expect(row("Timeout").getByText("Inherited")).toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByRole("spinbutton", { name: "Timeout" })).toHaveValue(
      30000,
    ),
  );
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  expect(ipc.saved.at(-1)?.options).toBeUndefined();
});

test("a changed option is written with the request", async () => {
  const ipc = setUp();
  await openSettings();
  await userEvent.click(screen.getByRole("radio", { name: "None" }));
  expect(
    row("Proxy").getByRole("button", { name: /Reset/ }),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  expect(ipc.saved.at(-1)?.options).toEqual({ useSystemProxy: false });
});

test("zero timeout means no limit", async () => {
  setUp();
  await openSettings();
  const timeout = screen.getByRole("spinbutton", { name: "Timeout" });
  await userEvent.clear(timeout);
  await userEvent.type(timeout, "0");
  expect(screen.getByText("No limit")).toBeInTheDocument();
});

test("turning TLS verification off warns in the editor", async () => {
  setUp();
  await openSettings();
  const banner =
    "TLS verification is off for this request. Anyone on the network path can read or change it.";
  expect(screen.queryByText(banner)).toBeNull();
  await userEvent.click(screen.getByRole("switch", { name: "Verify TLS" }));
  expect(screen.getByText(banner)).toBeInTheDocument();
  expect(
    screen.getByText(
      "Off. Anyone on the network path can read or change these requests",
    ),
  ).toBeInTheDocument();
});

test("a request inheriting a folder that skips TLS warns too", async () => {
  setUp({}, { verifyTls: false });
  openRequestTab(BY_ID);
  expect(
    await screen.findByText(/TLS verification is off for this request/),
  ).toBeInTheDocument();
});

test("cookies are a request setting", async () => {
  setUp();
  await openSettings();
  expect(
    screen.getByRole("switch", { name: "Use the cookie jar" }),
  ).toBeChecked();
});

test("a number can be retyped without snapping to the inherited value", async () => {
  const ipc = setUp({ options: { timeoutMs: 500 } });
  await openSettings();
  const timeout = screen.getByRole("spinbutton", { name: "Timeout" });
  await userEvent.clear(timeout);
  // Empty while typing: the field stays empty rather than showing 30000.
  expect(timeout).toHaveValue(null);
  await userEvent.type(timeout, "8000");
  expect(timeout).toHaveValue(8000);
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  expect(ipc.saved.at(-1)?.options).toEqual({ timeoutMs: 8000 });
});

test("a field left empty goes back to inherited", async () => {
  setUp({ options: { timeoutMs: 500 } });
  await openSettings();
  const timeout = screen.getByRole("spinbutton", { name: "Timeout" });
  await userEvent.clear(timeout);
  await userEvent.tab();
  await waitFor(() => expect(timeout).toHaveValue(30000));
  expect(row("Timeout").getByText("Inherited")).toBeInTheDocument();
});
