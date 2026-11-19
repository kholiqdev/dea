import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { SecretInput } from "$lib/ipc/bindings/SecretInput";
import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
import { reactiveVault, type ReactiveVault } from "./fake-vault.svelte";
import { addSecret } from "./state/add-secret.svelte";
import { resetSecrets } from "./state/secrets.svelte";
import VaultSidebar from "./components/VaultSidebar.svelte";
import VaultView from "./VaultView.svelte";

const NOW = new Date("2026-11-19T12:00:00Z");
const VALUE = "sk-live-0123456789";

let rows: SecretRow[];
let calls: { cmd: string; args: Record<string, unknown> }[];
let saveError: unknown;
let refreshed: ProjectInfo[];
let vault: ReactiveVault;

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
    environments: [
      { name: "local", production: false },
      { name: "staging", production: false },
      { name: "production", production: true },
    ],
    secretNames: [],
  };
}

function saved(): SecretInput {
  const call = calls.find((c) => c.cmd === "vault_save_secret");
  if (!call) throw new Error("nothing was saved");
  return call.args.input as SecretInput;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"], now: NOW });
  resetSecrets();
  addSecret.close();
  rows = [
    {
      name: "deploy_key",
      kind: "sshKey",
      description: "Deploy to the box",
      environment: "local",
      missing: true,
      updatedAt: null,
      expiresAt: null,
    },
  ];
  calls = [];
  saveError = undefined;
  refreshed = [];
  vault = reactiveVault();
  setProjectApi({
    info: project(),
    environment: null,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: (i) => refreshed.push(i),
  });
  setShellApi({
    openTab: () => "t",
    findTab: () => undefined,
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    toast: () => {},
    runCommand: () => Promise.resolve(),
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = (raw ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (cmd === "vault_secrets") return rows;
    if (cmd === "vault_save_secret") {
      if (saveError !== undefined)
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw saveError;
      const input = args.input as SecretInput;
      rows = [
        ...rows.filter(
          (r) =>
            !(r.name === input.name && r.environment === input.environment),
        ),
        {
          name: input.name,
          kind: input.kind,
          description: input.description,
          environment: input.environment,
          missing: false,
          updatedAt: "2026-11-19T12:00:00Z",
          expiresAt: input.expiresAt,
        },
      ];
      return { ...project(), secretNames: [input.name] };
    }
    return null;
  });
});

afterEach(() => {
  vi.useRealTimers();
});

async function openForm(user: ReturnType<typeof userEvent.setup>) {
  render(VaultSidebar);
  render(VaultView);
  await user.click(await screen.findByRole("button", { name: "Add secret" }));
}

test("saving a token declares it and shows it in the table", async () => {
  const user = userEvent.setup();
  await openForm(user);
  expect(
    screen.getByRole("heading", { name: "Add a secret" }),
  ).toBeInTheDocument();
  expect(
    screen.getByText(
      "Saved encrypted on this device, for the local environment of harbor-api",
    ),
  ).toBeInTheDocument();
  expect(screen.queryByRole("table")).not.toBeInTheDocument();

  await user.type(screen.getByRole("textbox", { name: "Name" }), "api_token");
  await user.type(screen.getByLabelText("Value"), VALUE);
  await user.type(
    screen.getByRole("textbox", { name: "Description" }),
    "Billing API",
  );
  await user.click(screen.getByRole("button", { name: "Save secret" }));

  const table = await screen.findByRole("table", { name: "Secrets" });
  expect(saved()).toEqual({
    name: "api_token",
    kind: "token",
    environment: "local",
    description: "Billing API",
    value: VALUE,
    refreshToken: null,
    expiresAt: null,
  });
  expect(refreshed).toHaveLength(1);
  expect(refreshed[0]?.secretNames).toEqual(["api_token"]);
  expect(
    screen.queryByRole("heading", { name: "Add a secret" }),
  ).not.toBeInTheDocument();
  // The table reloads after the save; the new row comes with it.
  expect(
    await within(table).findByRole("button", { name: "api_token" }),
  ).toHaveAttribute("aria-current", "true");
  expect(document.body.textContent).not.toContain(VALUE);
});

test("an OAuth token takes a refresh token and an expiry", async () => {
  const user = userEvent.setup();
  await openForm(user);
  expect(screen.queryByLabelText("Refresh token")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Expires")).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("Type"), "OAuth token");
  await user.type(screen.getByRole("textbox", { name: "Name" }), "google");
  await user.type(screen.getByLabelText("Value"), "access");
  await user.type(screen.getByLabelText("Refresh token"), "refresh");
  await user.type(screen.getByLabelText("Expires"), "2026-12-01T10:30");
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await screen.findByRole("table", { name: "Secrets" });
  expect(saved()).toMatchObject({
    kind: "oauthToken",
    refreshToken: "refresh",
    // The field is local time; the backend gets UTC.
    expiresAt: new Date("2026-12-01T10:30").toISOString(),
  });
});

test("another type sends no refresh token or expiry", async () => {
  const user = userEvent.setup();
  await openForm(user);
  await user.selectOptions(screen.getByLabelText("Type"), "OAuth token");
  await user.type(screen.getByLabelText("Refresh token"), "refresh");
  await user.selectOptions(screen.getByLabelText("Type"), "Password");
  await user.type(screen.getByRole("textbox", { name: "Name" }), "db");
  await user.type(screen.getByLabelText("Value"), "pw");
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await screen.findByRole("table", { name: "Secrets" });
  expect(saved()).toMatchObject({
    kind: "password",
    refreshToken: null,
    expiresAt: null,
  });
});

test("Add value fills and locks the name and type", async () => {
  const user = userEvent.setup();
  render(VaultSidebar);
  render(VaultView);
  const row = (
    await screen.findByRole("button", { name: "deploy_key" })
  ).closest("tr");
  if (!row) throw new Error("no row");
  await user.click(within(row).getByRole("button", { name: "Add value" }));
  const name = screen.getByRole("textbox", { name: "Name" });
  expect(name).toHaveValue("deploy_key");
  expect(name).toBeDisabled();
  const type = screen.getByLabelText("Type");
  expect(type).toHaveValue("sshKey");
  expect(type).toBeDisabled();
  expect(screen.getByLabelText("Environment")).toHaveValue("local");
  // The declared description stays unless the user changes it.
  expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue(
    "Deploy to the box",
  );
  await user.type(screen.getByLabelText("Value"), "key-material");
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await screen.findByRole("table", { name: "Secrets" });
  expect(saved()).toMatchObject({
    name: "deploy_key",
    kind: "sshKey",
    environment: "local",
    description: "Deploy to the box",
    value: "key-material",
  });
});

test("problems show before saving", async () => {
  const user = userEvent.setup();
  await openForm(user);
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  expect(screen.getByText("A secret needs a name")).toBeInTheDocument();
  expect(screen.getByText("A secret needs a value")).toBeInTheDocument();
  await user.type(screen.getByRole("textbox", { name: "Name" }), "1bad name");
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  expect(
    screen.getByText(
      '"1bad name" is not a secret name: start with a letter or _, then use letters, digits, _, . or -',
    ),
  ).toBeInTheDocument();
  expect(calls.some((c) => c.cmd === "vault_save_secret")).toBe(false);
});

test("a locked Vault asks to unlock and keeps the form", async () => {
  const user = userEvent.setup();
  await openForm(user);
  await user.type(screen.getByRole("textbox", { name: "Name" }), "api_token");
  await user.type(screen.getByLabelText("Value"), VALUE);
  vault.locked = true;
  const unlock = vi.spyOn(vault, "unlock").mockResolvedValue(false);
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await waitFor(() => expect(unlock).toHaveBeenCalledTimes(1));
  expect(calls.some((c) => c.cmd === "vault_save_secret")).toBe(false);
  expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue(
    "api_token",
  );
  expect(screen.getByLabelText("Value")).toHaveValue(VALUE);

  unlock.mockRestore();
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await screen.findByRole("table", { name: "Secrets" });
  expect(saved().value).toBe(VALUE);
});

test("an error from Dea is shown and the form stays", async () => {
  const user = userEvent.setup();
  saveError = { kind: "readOnly", message: "this project is read-only" };
  await openForm(user);
  await user.type(screen.getByRole("textbox", { name: "Name" }), "api_token");
  await user.type(screen.getByLabelText("Value"), VALUE);
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "This project is read-only",
  );
  expect(screen.getByLabelText("Value")).toHaveValue(VALUE);
  expect(refreshed).toHaveLength(0);
  saveError = {
    kind: "mergeConflict",
    message: "vault.yaml has conflict markers",
  };
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await waitFor(() =>
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Vault.yaml has conflict markers",
    ),
  );
  expect(screen.getByLabelText("Value")).toHaveValue(VALUE);
});

test("Cancel closes the form and drops the value", async () => {
  const user = userEvent.setup();
  await openForm(user);
  await user.type(screen.getByLabelText("Value"), VALUE);
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(await screen.findByRole("table")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Add secret" }));
  expect(screen.getByLabelText("Value")).toHaveValue("");
  expect(calls.some((c) => c.cmd === "vault_save_secret")).toBe(false);
});

test("Reveal shows the typed value for 30 s, then masks it", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true, now: NOW });
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  await openForm(user);
  const value = screen.getByLabelText("Value");
  await user.type(value, VALUE);
  expect(value).toHaveAttribute("type", "password");
  await user.click(screen.getByRole("button", { name: "Reveal" }));
  expect(value).toHaveAttribute("type", "text");
  await vi.advanceTimersByTimeAsync(29_000);
  expect(value).toHaveAttribute("type", "text");
  await vi.advanceTimersByTimeAsync(1_000);
  expect(value).toHaveAttribute("type", "password");
  expect(value).toHaveValue(VALUE);
  expect(calls.some((c) => c.cmd === "vault_reveal")).toBe(false);
});

test("the form has Reveal but no Copy", async () => {
  const user = userEvent.setup();
  await openForm(user);
  expect(screen.getByRole("button", { name: "Reveal" })).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Copy" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByText("Reveal hides the value again after 30 seconds"),
  ).toBeInTheDocument();
});

test("Reveal is off while the Vault is locked", async () => {
  const user = userEvent.setup();
  await openForm(user);
  const value = screen.getByLabelText("Value");
  await user.type(value, VALUE);
  const reveal = screen.getByRole("button", { name: "Reveal" });
  expect(reveal).toBeEnabled();

  vault.locked = true;
  await waitFor(() => expect(reveal).toBeDisabled());
  await user.click(reveal);
  expect(value).toHaveAttribute("type", "password");

  // A value shown when the Vault locks is masked at once.
  vault.locked = false;
  await waitFor(() => expect(reveal).toBeEnabled());
  await user.click(reveal);
  expect(value).toHaveAttribute("type", "text");
  vault.locked = true;
  await waitFor(() => expect(value).toHaveAttribute("type", "password"));
});

test("saving over a value asks for a second press", async () => {
  rows.push({
    name: "api_token",
    kind: "token",
    description: null,
    environment: "local",
    missing: false,
    updatedAt: "2026-11-18T12:00:00Z",
    expiresAt: null,
  });
  const user = userEvent.setup();
  await openForm(user);
  const warning =
    '"api_token" already has a value in local. Saving replaces it.';
  await user.type(screen.getByRole("textbox", { name: "Name" }), "api_token");
  await user.type(screen.getByLabelText("Value"), VALUE);
  expect(screen.getByText(warning)).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Save secret" }));
  expect(calls.some((c) => c.cmd === "vault_save_secret")).toBe(false);
  expect(
    screen.queryByRole("button", { name: "Save secret" }),
  ).not.toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Replace value" }));
  await screen.findByRole("table", { name: "Secrets" });
  expect(saved()).toMatchObject({
    name: "api_token",
    environment: "local",
    value: VALUE,
  });
});

test("another environment without a value saves at once", async () => {
  rows.push({
    name: "api_token",
    kind: "token",
    description: null,
    environment: "local",
    missing: false,
    updatedAt: "2026-11-18T12:00:00Z",
    expiresAt: null,
  });
  const user = userEvent.setup();
  await openForm(user);
  await user.type(screen.getByRole("textbox", { name: "Name" }), "api_token");
  await user.type(screen.getByLabelText("Value"), VALUE);
  await user.selectOptions(screen.getByLabelText("Environment"), "staging");
  expect(screen.queryByText(/already has a value/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Save secret" }));
  await screen.findByRole("table", { name: "Secrets" });
  expect(saved()).toMatchObject({ name: "api_token", environment: "staging" });
});

test("a declared name brings its type and description", async () => {
  setProjectApi({
    info: { ...project(), secretNames: ["deploy_key"] },
    environment: null,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: (i) => refreshed.push(i),
  });
  const user = userEvent.setup();
  await openForm(user);
  await user.type(screen.getByRole("textbox", { name: "Name" }), "deploy_key");
  expect(screen.getByLabelText("Type")).toHaveValue("sshKey");
  expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue(
    "Deploy to the box",
  );
});

test("a declared name keeps a type and description already typed", async () => {
  setProjectApi({
    info: { ...project(), secretNames: ["deploy_key"] },
    environment: null,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: (i) => refreshed.push(i),
  });
  const user = userEvent.setup();
  await openForm(user);
  await user.selectOptions(screen.getByLabelText("Type"), "Password");
  await user.type(screen.getByRole("textbox", { name: "Description" }), "Mine");
  await user.type(screen.getByRole("textbox", { name: "Name" }), "deploy_key");
  expect(screen.getByLabelText("Type")).toHaveValue("password");
  expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue(
    "Mine",
  );
});
