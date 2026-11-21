// Renaming or deleting an environment takes its Vault values along, which
// needs the Vault unlocked: the editor unlocks and tries once more.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { setProjectApi, type ProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import { setVaultApi } from "$app/contributions/vault-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import EnvironmentsTab from "./EnvironmentsTab.svelte";
import { resetEnvironmentsEditor } from "./state/environments.svelte";
import { info as baseInfo } from "./test-support";

let project: ProjectInfo;
let saved: { previous: string | null; environment: Environment }[];
let deleted: string[];
/** Whether the Vault is unlocked, as Dea sees it. */
let unlocked: boolean;
/** How many times the editor asked to unlock. */
let unlocks: number;
/** What unlock answers. */
let answer: boolean;

const RENAME_LOCKED =
  "unlock the Vault to rename an environment that has secrets";
const DELETE_LOCKED =
  "unlock the Vault to delete an environment that has secrets";

function locked(message: string): never {
  // eslint-disable-next-line @typescript-eslint/only-throw-error -- commands reject with { kind, message }
  throw { kind: "vaultLocked", message };
}

beforeEach(() => {
  project = {
    ...baseInfo("harbor-api", "/code/harbor-api"),
    environments: [
      { name: "local", production: false },
      { name: "staging", production: false },
    ],
  };
  saved = [];
  deleted = [];
  unlocked = false;
  unlocks = 0;
  answer = true;
  resetEnvironmentsEditor();
  const api: ProjectApi = {
    get info() {
      return project;
    },
    environment: null,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: (info) => {
      project = info;
    },
  };
  setProjectApi(api);
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
    toast: () => 0,
    dismissToast: () => {},
    confirmingSend: false,
    runCommand: () => Promise.resolve(),
  });
  setVaultApi({
    get locked() {
      return !unlocked;
    },
    method: "deviceOwner",
    unlock: () => {
      unlocks += 1;
      unlocked = answer;
      return Promise.resolve(answer);
    },
    lock: () => Promise.resolve(),
    authorize: () => Promise.resolve(true),
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = raw as Record<string, unknown>;
    if (cmd === "save_environment") {
      const environment = args.environment as Environment;
      const previous = args.previous as string | null;
      // staging has secrets on this device.
      if (previous === "staging" && environment.name !== previous && !unlocked)
        locked(RENAME_LOCKED);
      saved.push({ previous, environment });
      project = {
        ...project,
        environments: project.environments.map((e) =>
          e.name === previous ? environment : e,
        ),
      };
      return project;
    }
    if (cmd === "environment_references") return [];
    if (cmd === "delete_environment") {
      if (args.name === "staging" && !unlocked) locked(DELETE_LOCKED);
      deleted.push(args.name as string);
      project = {
        ...project,
        environments: project.environments.filter((e) => e.name !== args.name),
      };
      return project;
    }
    return null;
  });
});

async function select(name: string) {
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
  await userEvent.click(
    within(screen.getByRole("navigation", { name: "Environments" })).getByRole(
      "button",
      { name: new RegExp(`^${name}`) },
    ),
  );
}

async function rename(to: string) {
  const name = screen.getByRole("textbox", { name: "Name" });
  await userEvent.clear(name);
  await userEvent.type(name, to);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
}

async function remove(name: string) {
  await userEvent.click(
    screen.getByRole("button", { name: `Delete ${name}…` }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: `Delete ${name}` }),
  );
}

test("renaming an environment with secrets unlocks the Vault, then saves", async () => {
  await select("staging");
  await rename("qa");
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(unlocks).toBe(1);
  expect(saved[0]).toMatchObject({
    previous: "staging",
    environment: { name: "qa" },
  });
  expect(screen.queryByText(/unlock the Vault/i)).toBeNull();
  expect(screen.queryByRole("img", { name: "Unsaved changes" })).toBeNull();
});

test("a rename whose unlock is cancelled saves nothing and says why", async () => {
  answer = false;
  await select("staging");
  await rename("qa");
  expect(
    await screen.findByText(
      "Unlock the Vault to rename an environment that has secrets",
    ),
  ).toBeInTheDocument();
  expect(unlocks).toBe(1);
  expect(saved).toEqual([]);
  expect(
    screen.getByRole("img", { name: "Unsaved changes" }),
  ).toBeInTheDocument();
});

test("a rename without secrets does not ask to unlock", async () => {
  await select("local");
  await rename("dev");
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(unlocks).toBe(0);
});

test("deleting an environment with secrets unlocks the Vault, then deletes", async () => {
  await select("staging");
  await remove("staging");
  await waitFor(() => expect(deleted).toEqual(["staging"]));
  expect(unlocks).toBe(1);
  expect(project.environments.map((e) => e.name)).toEqual(["local"]);
});

test("a delete whose unlock is cancelled deletes nothing and says why", async () => {
  answer = false;
  await select("staging");
  await remove("staging");
  expect(
    await screen.findByText(
      "Unlock the Vault to delete an environment that has secrets",
    ),
  ).toBeInTheDocument();
  expect(unlocks).toBe(1);
  expect(deleted).toEqual([]);
  expect(project.environments.map((e) => e.name)).toContain("staging");
});

test("the write is tried once more, not again and again", async () => {
  // The Vault locks again before the second try reaches it.
  setVaultApi({
    locked: true,
    method: "deviceOwner",
    unlock: () => {
      unlocks += 1;
      return Promise.resolve(true);
    },
    lock: () => Promise.resolve(),
    authorize: () => Promise.resolve(true),
  });
  await select("staging");
  await remove("staging");
  expect(
    await screen.findByText(
      "Unlock the Vault to delete an environment that has secrets",
    ),
  ).toBeInTheDocument();
  expect(unlocks).toBe(1);
  expect(deleted).toEqual([]);
});
