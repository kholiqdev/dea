// Secret names in the command palette (F1-FON-11 #2, #6, #7).

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { beforeEach, expect, test } from "vitest";
import { setProjectApi, type ProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
import { manifest } from "./index";
import secretNames from "./search";
import { resetSecrets, secrets } from "./state/secrets.svelte";
import { reactiveVault } from "./fake-vault.svelte";

function row(environment: string, name: string, missing = false): SecretRow {
  return {
    name,
    kind: "token",
    description: null,
    environment,
    missing,
    updatedAt: missing ? null : "2026-11-19T10:00:00Z",
    expiresAt: null,
  };
}

let calls: string[];
let opened: string[];

beforeEach(() => {
  resetSecrets();
  calls = [];
  opened = [];
  const info = {
    name: "harbor-api",
    environments: [
      { name: "staging", production: false },
      { name: "production", production: true },
    ],
  } as unknown as ProjectInfo;
  const project: ProjectApi = {
    info,
    environment: null,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: () => {},
  };
  setProjectApi(project);
  setShellApi({
    openTab: () => "t",
    findTab: () => undefined,
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    setSaveHandler: () => {},
    openModule: (id) => {
      opened.push(id);
      return Promise.resolve();
    },
    runCommand: () => Promise.resolve(),
    toast: () => {},
  });
  clearMocks();
  mockIPC((cmd) => {
    calls.push(cmd);
    if (cmd === "vault_secrets")
      return [
        row("production", "access_token"),
        row("staging", "access_token"),
        row("staging", "pg_password", true),
      ];
    return null;
  });
});

test("the Vault module lists secret names in the palette", () => {
  expect(manifest.searchSources).toEqual([
    expect.objectContaining({
      id: "vault.secrets",
      group: "Secret names",
      order: 70,
    }),
  ]);
});

test("secret names show while locked and never read a value", async () => {
  const vault = reactiveVault();
  await vault.lock();
  const items = await secretNames();
  expect(items.map((i) => [i.title, i.meta, i.environment])).toEqual([
    [
      "access_token",
      "Name only, values never show here",
      { name: "production", production: true },
    ],
    [
      "access_token",
      "Name only, values never show here",
      { name: "staging", production: false },
    ],
  ]);
  expect(calls).toEqual(["vault_secrets"]);
  expect(vault.locked).toBe(true);
});

test("choosing a secret name opens the Vault on it", async () => {
  reactiveVault();
  const items = await secretNames();
  await items[0]?.open("active");
  expect(secrets.chosen).toBe("production");
  expect(secrets.choice).toEqual({
    environment: "production",
    name: "access_token",
  });
  expect(opened).toEqual(["vault"]);
});

test("Lock Vault locks with Mod+L", async () => {
  const vault = reactiveVault();
  expect(manifest.commands).toContainEqual({
    id: "vault.lock",
    title: "Lock Vault",
    action: { type: "intent", intent: "vault.lock" },
  });
  expect(manifest.shortcuts).toContainEqual({
    command: "vault.lock",
    chord: "Mod+L",
  });
  const intent = manifest.intents?.find((i) => i.intent === "vault.lock");
  const { default: run } = await intent!.handler();
  await run(undefined);
  expect(vault.locks).toBe(1);
});
