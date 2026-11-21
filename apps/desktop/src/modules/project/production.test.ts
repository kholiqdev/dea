// Turning production off, renaming it or deleting it asks to sign in first
// (F1-FON-07 #4).

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
/** Every action authorize was asked for. */
let asked: string[];
/** What authorize answers. */
let answer: boolean;
/** The command a refused write fails with, once the window has expired. */
let expired: boolean;

beforeEach(() => {
  project = {
    ...baseInfo("harbor-api", "/code/harbor-api"),
    environments: [
      { name: "local", production: false },
      { name: "production", production: true },
    ],
  };
  saved = [];
  deleted = [];
  asked = [];
  answer = true;
  expired = false;
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
    runCommand: () => Promise.resolve(),
  });
  setVaultApi({
    locked: false,
    method: "deviceOwner",
    unlock: () => Promise.resolve(true),
    lock: () => Promise.resolve(),
    authorize: (action) => {
      asked.push(action);
      return Promise.resolve(answer);
    },
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = raw as Record<string, unknown>;
    if (cmd === "save_environment") {
      const environment = args.environment as Environment;
      const previous = args.previous as string | null;
      if (expired)
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw {
          kind: "reauthRequired",
          message: "turning production off needs authentication first",
        };
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

const prodSwitch = () =>
  screen.getByRole("switch", { name: "Treat as production" });

test("turning production off asks first and stays on when cancelled", async () => {
  answer = false;
  await select("production");
  expect(prodSwitch()).toBeEnabled();
  await userEvent.click(prodSwitch());
  expect(asked).toEqual(["turn off production for production"]);
  expect(prodSwitch()).toHaveAttribute("aria-checked", "true");
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
});

test("turning production off after authentication saves false", async () => {
  await select("production");
  await userEvent.click(prodSwitch());
  await waitFor(() =>
    expect(prodSwitch()).toHaveAttribute("aria-checked", "false"),
  );
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0]?.environment.production).toBe(false);
});

test("a save refused for authentication keeps the draft and says why", async () => {
  await select("production");
  await userEvent.click(prodSwitch());
  await waitFor(() =>
    expect(prodSwitch()).toHaveAttribute("aria-checked", "false"),
  );
  expired = true;
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(
    await screen.findByText(
      "Turning production off needs authentication first",
    ),
  ).toBeInTheDocument();
  expect(prodSwitch()).toHaveAttribute("aria-checked", "false");
  expect(
    screen.getByRole("img", { name: "Unsaved changes" }),
  ).toBeInTheDocument();
});

test("turning production on or off for another environment never asks", async () => {
  await select("local");
  await userEvent.click(prodSwitch());
  await userEvent.click(prodSwitch());
  expect(asked).toEqual([]);
});

test("renaming a production environment asks first", async () => {
  await select("production");
  const name = screen.getByRole("textbox", { name: "Name" });
  await userEvent.clear(name);
  await userEvent.type(name, "live");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(asked).toEqual(["rename the production environment production"]);
  expect(saved[0]?.previous).toBe("production");
  expect(saved[0]?.environment.name).toBe("live");
});

test("cancelling the rename of a production environment saves nothing", async () => {
  answer = false;
  await select("production");
  const name = screen.getByRole("textbox", { name: "Name" });
  await userEvent.clear(name);
  await userEvent.type(name, "live");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(asked).toHaveLength(1));
  expect(saved).toEqual([]);
  expect(
    screen.getByRole("img", { name: "Unsaved changes" }),
  ).toBeInTheDocument();
  expect(screen.queryByText(/authentication/i)).toBeNull();
});

test("deleting a production environment asks first", async () => {
  await select("production");
  await userEvent.click(
    screen.getByRole("button", { name: "Delete production…" }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Delete production" }),
  );
  await waitFor(() => expect(deleted).toEqual(["production"]));
  expect(asked).toEqual(["delete the production environment production"]);
});

test("cancelling the delete of a production environment deletes nothing", async () => {
  answer = false;
  await select("production");
  await userEvent.click(
    screen.getByRole("button", { name: "Delete production…" }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Delete production" }),
  );
  await waitFor(() => expect(asked).toHaveLength(1));
  expect(deleted).toEqual([]);
  expect(project.environments.map((e) => e.name)).toContain("production");
});

test("the environment asked about is the one turned off, whatever is selected", async () => {
  let finish: (ok: boolean) => void = () => {};
  setVaultApi({
    locked: false,
    method: "deviceOwner",
    unlock: () => Promise.resolve(true),
    lock: () => Promise.resolve(),
    authorize: (action) => {
      asked.push(action);
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  });
  await select("production");
  await userEvent.click(prodSwitch());
  // A second click while the question is open asks nothing more.
  await userEvent.click(prodSwitch());
  expect(asked).toEqual(["turn off production for production"]);

  // The person moves to local before answering.
  const nav = screen.getByRole("navigation", { name: "Environments" });
  await userEvent.click(within(nav).getByRole("button", { name: /^local/ }));
  finish(true);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled(),
  );
  expect(prodSwitch()).toHaveAttribute("aria-checked", "false");

  await userEvent.click(
    within(nav).getByRole("button", { name: /^production/ }),
  );
  expect(prodSwitch()).toHaveAttribute("aria-checked", "false");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0]).toMatchObject({
    previous: "production",
    environment: { name: "production", production: false },
  });
});
