import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { flushSync } from "svelte";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
import { reactiveVault, type ReactiveVault } from "./fake-vault.svelte";
import { resetSecrets, usedBy } from "./state/secrets.svelte";
import VaultSidebar from "./components/VaultSidebar.svelte";
import VaultView from "./VaultView.svelte";

const NOW = new Date("2026-11-19T12:00:00Z");
const VALUE = "sk-live-0123456789";

function row(
  environment: string,
  name: string,
  over: Partial<SecretRow> = {},
): SecretRow {
  return {
    name,
    kind: "token",
    description: null,
    environment,
    missing: false,
    updatedAt: "2026-11-19T11:52:00Z",
    expiresAt: null,
    ...over,
  };
}

const ENVIRONMENTS: Environment[] = [
  { name: "local", production: false },
  {
    name: "staging",
    production: false,
    variables: [
      { name: "authHeader", value: "Bearer {{vault:access_token}}" },
      { name: "rawToken", value: "{{vault:access_token}}" },
      { name: "dbPass", value: "{{ vault: db_password }}" },
    ],
  },
  { name: "production", production: true },
];

let rows: SecretRow[];
let calls: { cmd: string; args: Record<string, unknown> }[];
let toasts: string[];
let vault: ReactiveVault;
let revealError: unknown;
let info: ProjectInfo;

function project(): ProjectInfo {
  return {
    root: "/code/harbor-api",
    name: "harbor-api",
    scope: "s",
    slot: 0,
    readOnly: false,
    defaultEnvironment: "local",
    environmentFiles: {},
    errors: [],
    migratedFrom: null,
    missing: false,
    environments: ENVIRONMENTS,
    secretNames: ["access_token", "db_password", "deploy_key"],
  };
}

function mount() {
  return [render(VaultSidebar), render(VaultView)];
}

beforeEach(() => {
  // Only the clock stands still: the table's ages and expiry depend on it.
  vi.useFakeTimers({ toFake: ["Date"], now: NOW });
  resetSecrets();
  info = project();
  rows = [
    row("local", "access_token"),
    row("staging", "access_token", { description: "API access" }),
    row("staging", "db_password", {
      kind: "password",
      updatedAt: "2026-11-07T12:00:00Z",
    }),
    row("staging", "deploy_key", {
      kind: "sshKey",
      missing: true,
      updatedAt: null,
    }),
    row("production", "access_token", {
      kind: "oauthToken",
      expiresAt: "2026-11-19T12:40:00Z",
    }),
  ];
  calls = [];
  toasts = [];
  revealError = undefined;
  vault = reactiveVault();
  setProjectApi({
    info,
    environment: null,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: () => {},
  });
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
    toast: (text) => toasts.push(text),
    dismissToast: () => {},
    confirmingSend: false,
    runCommand: () => Promise.resolve(),
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = (raw ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    switch (cmd) {
      case "vault_secrets":
        return rows;
      case "vault_reveal":
        if (revealError !== undefined)
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw revealError;
        return VALUE;
      case "vault_delete_value": {
        rows = rows.map((r) =>
          r.environment === args.environment && r.name === args.name
            ? { ...r, missing: true, updatedAt: null }
            : r,
        );
        return null;
      }
      default:
        return null;
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
});

async function openStaging(user: ReturnType<typeof userEvent.setup>) {
  mount();
  await user.click(await screen.findByRole("button", { name: /^staging/ }));
}

function tableRow(name: string): HTMLElement {
  const cell = screen.getByRole("button", { name });
  const tr = cell.closest("tr");
  if (!tr) throw new Error("no row");
  return tr;
}

test("the table lists each declared secret of the chosen environment", async () => {
  const user = userEvent.setup();
  await openStaging(user);
  expect(
    await screen.findByRole("heading", { name: "staging", level: 1 }),
  ).toBeInTheDocument();
  expect(
    screen.getByText("3 secrets in harbor-api, 1 is missing a value"),
  ).toBeInTheDocument();
  const table = screen.getByRole("table", { name: "Secrets" });
  expect(
    within(table)
      .getAllByRole("columnheader")
      .map((h) => h.textContent),
  ).toEqual(["Name", "Type", "Used by", "Expires", "Updated"]);
  expect(within(table).getAllByRole("row")).toHaveLength(4);
  const missing = tableRow("deploy_key");
  expect(within(missing).getByText("Missing")).toBeInTheDocument();
  expect(
    within(missing).getByRole("button", { name: "Add value" }),
  ).toBeInTheDocument();
  expect(
    within(tableRow("access_token")).getByText("Token"),
  ).toBeInTheDocument();
  expect(within(missing).getByText("SSH key")).toBeInTheDocument();
  expect(
    within(tableRow("db_password")).getByText("12 days ago"),
  ).toBeInTheDocument();
  // The sidebar counts the values this device has.
  expect(screen.getByRole("button", { name: /^staging/ })).toHaveTextContent(
    "2",
  );
  expect(screen.getByRole("button", { name: /^production/ })).toHaveTextContent(
    "1",
  );
});

test("one secret reads in the singular", async () => {
  const user = userEvent.setup();
  mount();
  await user.click(await screen.findByRole("button", { name: /^local/ }));
  expect(await screen.findByText("1 secret in harbor-api")).toBeInTheDocument();
});

test("type tabs filter the table", async () => {
  const user = userEvent.setup();
  await openStaging(user);
  await screen.findByRole("table", { name: "Secrets" });
  const tabs = screen.getByRole("tablist", { name: "Type" });
  expect(
    within(tabs)
      .getAllByRole("tab")
      .map((t) => t.textContent),
  ).toEqual([
    "All types",
    "Tokens",
    "Passwords",
    "SSH keys",
    "API keys",
    "OAuth tokens",
  ]);
  await user.click(within(tabs).getByRole("tab", { name: "Passwords" }));
  const table = screen.getByRole("table", { name: "Secrets" });
  expect(within(table).getAllByRole("row")).toHaveLength(2);
  expect(within(table).getByText("db_password")).toBeInTheDocument();
  await user.click(within(tabs).getByRole("tab", { name: "All types" }));
  expect(within(table).getAllByRole("row")).toHaveLength(4);
});

test("Used by counts the variables that refer to the secret", async () => {
  const user = userEvent.setup();
  await openStaging(user);
  await screen.findByRole("table", { name: "Secrets" });
  expect(
    within(tableRow("access_token")).getByText("2 variables"),
  ).toBeInTheDocument();
  expect(
    within(tableRow("db_password")).getByText("1 variable"),
  ).toBeInTheDocument();
  expect(
    within(tableRow("deploy_key")).getAllByRole("cell")[2],
  ).toHaveTextContent("—");

  expect(usedBy(info, "staging", "access_token")).toEqual([
    "authHeader",
    "rawToken",
  ]);
  // Only the environment's own variables count.
  expect(usedBy(info, "local", "access_token")).toEqual([]);

  await user.click(screen.getByRole("button", { name: "access_token" }));
  const inspector = screen.getByRole("complementary", { name: "Secret" });
  expect(
    within(inspector).getByRole("heading", { name: "access_token" }),
  ).toBeInTheDocument();
  expect(
    within(inspector).getByText("Token in harbor-api / staging"),
  ).toBeInTheDocument();
  expect(within(inspector).getByText("authHeader")).toBeInTheDocument();
  expect(within(inspector).getAllByText("Variable in staging")).toHaveLength(2);
});

test("a revealed value hides on lock, on another secret and after 30 s", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true, now: NOW });
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  await openStaging(user);
  await screen.findByRole("table", { name: "Secrets" });

  const reveal = async (name: string) => {
    await user.click(screen.getByRole("button", { name }));
    await user.click(screen.getByRole("button", { name: "Reveal value" }));
    expect(await screen.findByText(VALUE)).toBeInTheDocument();
  };

  // After 30 s.
  await reveal("access_token");
  expect(calls.at(-1)).toEqual({
    cmd: "vault_reveal",
    args: { environment: "staging", name: "access_token" },
  });
  vi.advanceTimersByTime(29_000);
  flushSync();
  expect(screen.getByText(VALUE)).toBeInTheDocument();
  vi.advanceTimersByTime(1_500);
  flushSync();
  expect(screen.queryByText(VALUE)).not.toBeInTheDocument();

  // On another secret.
  await reveal("access_token");
  await user.click(screen.getByRole("button", { name: "db_password" }));
  expect(screen.queryByText(VALUE)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "access_token" }));
  expect(screen.queryByText(VALUE)).not.toBeInTheDocument();

  // On lock, and still hidden once unlocked again.
  await reveal("access_token");
  vault.locked = true;
  flushSync();
  expect(screen.queryByText(VALUE)).not.toBeInTheDocument();
  vault.locked = false;
  flushSync();
  await user.click(await screen.findByRole("button", { name: "access_token" }));
  expect(screen.queryByText(VALUE)).not.toBeInTheDocument();
  expect(document.body.textContent).not.toContain(VALUE);
});

test("leaving the module hides a revealed value", async () => {
  const user = userEvent.setup();
  const view = mount()[1];
  if (!view) throw new Error("no view");
  await user.click(await screen.findByRole("button", { name: /^staging/ }));
  await user.click(await screen.findByRole("button", { name: "access_token" }));
  await user.click(screen.getByRole("button", { name: "Reveal value" }));
  expect(await screen.findByText(VALUE)).toBeInTheDocument();
  view.unmount();
  expect(document.body.textContent).not.toContain(VALUE);
});

test("Copy says the clipboard clears in 30 s", async () => {
  const user = userEvent.setup();
  await openStaging(user);
  await user.click(await screen.findByRole("button", { name: "access_token" }));
  expect(
    screen.getByText("Copied values clear from the clipboard after 30 s"),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Copy value" }));
  await waitFor(() =>
    expect(toasts).toEqual(["Copied. The clipboard clears in 30 s"]),
  );
  expect(calls.at(-1)).toEqual({
    cmd: "vault_copy",
    args: { environment: "staging", name: "access_token" },
  });
});

test("a cancelled production check reveals nothing", async () => {
  const user = userEvent.setup();
  mount();
  await user.click(await screen.findByRole("button", { name: /^production/ }));
  expect(await screen.findByText("Expires in 40 min")).toBeInTheDocument();
  await user.click(await screen.findByRole("button", { name: "access_token" }));
  revealError = { kind: "cancelled", message: "cancelled" };
  await user.click(screen.getByRole("button", { name: "Reveal value" }));
  await waitFor(() =>
    expect(calls.some((c) => c.cmd === "vault_reveal")).toBe(true),
  );
  await new Promise((r) => setTimeout(r, 20));
  expect(screen.queryByText(VALUE)).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Reveal value" }),
  ).toBeInTheDocument();
  expect(toasts).toEqual([]);

  // Another failure does say so.
  revealError = { kind: "vaultLocked", message: "the vault is locked" };
  await user.click(screen.getByRole("button", { name: "Reveal value" }));
  await waitFor(() => expect(toasts).toEqual(["The vault is locked"]));
});

test("deleting a value asks first and leaves the secret Missing", async () => {
  const user = userEvent.setup();
  await openStaging(user);
  await user.click(await screen.findByRole("button", { name: "db_password" }));
  await user.click(screen.getByRole("button", { name: "Delete value…" }));
  const dialog = screen.getByRole("dialog", {
    name: "Delete the value of db_password in staging?",
  });
  expect(
    within(dialog).getByText(
      "Removes the value from this device. vault.yaml still lists the secret, so it shows as Missing.",
    ),
  ).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  expect(calls.some((c) => c.cmd === "vault_delete_value")).toBe(false);

  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(calls.some((c) => c.cmd === "vault_delete_value")).toBe(false);

  await user.click(screen.getByRole("button", { name: "Delete value…" }));
  await user.click(screen.getByRole("button", { name: "Delete" }));
  await waitFor(() =>
    expect(
      within(tableRow("db_password")).getByText("Missing"),
    ).toBeInTheDocument(),
  );
  expect(calls.find((c) => c.cmd === "vault_delete_value")?.args).toEqual({
    environment: "staging",
    name: "db_password",
  });
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    screen.getByText("3 secrets in harbor-api, 2 are missing a value"),
  ).toBeInTheDocument();
});

test("Lock now locks the Vault", async () => {
  const user = userEvent.setup();
  await openStaging(user);
  await user.click(await screen.findByRole("button", { name: "Lock now" }));
  expect(vault.locks).toBe(1);
  expect(
    await screen.findByRole("heading", { name: "Vault is locked" }),
  ).toBeInTheDocument();
});
