// Saving in the environment editor keeps whatever it did not write.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { beforeEach, expect, test } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import EnvironmentsTab from "./EnvironmentsTab.svelte";
import { FakeProject } from "./fake-project.svelte";
import editEnvironments from "./intents/edit-environments";
import {
  environmentsEditor,
  resetEnvironmentsEditor,
} from "./state/environments.svelte";
import { info as baseInfo } from "./test-support";

function env(name: string, baseUrl: string): Environment {
  return {
    name,
    production: false,
    variables: [{ name: "baseUrl", value: baseUrl }],
  };
}

function projectWith(environments: Environment[]): ProjectInfo {
  return {
    ...baseInfo("harbor-api", "/code/harbor-api"),
    environments,
    environmentFiles: Object.fromEntries(
      environments.map((e) => [e.name, `environments/${e.name}.yaml`]),
    ),
  };
}

let project: FakeProject;
let saves: { previous: string | null; environment: Environment }[];
/** Names save_environment refuses as taken. */
let taken: string[];
let opened: string[];

beforeEach(() => {
  resetEnvironmentsEditor();
  project = new FakeProject(
    projectWith([
      env("local", "http://localhost:8080"),
      env("staging", "https://staging.example.com"),
    ]),
  );
  setProjectApi(project);
  saves = [];
  taken = [];
  opened = [];
  setShellApi({
    openTab: (type) => {
      opened.push(type);
      return "t";
    },
    findTab: () => undefined,
    updateTab: () => {},
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    activeTab: () => null,
    isTabDirty: () => false,
    setSaveHandler: () => {},
    showSidebar: () => {},
    openModule: () => Promise.resolve(),
    toast: () => 0,
    dismissToast: () => {},
    confirmingSend: false,
    runCommand: () => Promise.resolve(),
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = raw as Record<string, unknown>;
    const envs = project.info?.environments ?? [];
    if (cmd === "save_environment") {
      const environment = args.environment as Environment;
      const previous = args.previous as string | null;
      if (taken.includes(environment.name))
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw {
          kind: "nameTaken",
          message: `an environment called "${environment.name}" already exists`,
        };
      saves.push({ previous, environment });
      project.info = projectWith([
        ...envs.filter(
          (e) => e.name !== previous && e.name !== environment.name,
        ),
        environment,
      ]);
      return project.info;
    }
    if (cmd === "duplicate_environment") {
      const source = envs.find((e) => e.name === args.name);
      project.info = projectWith([
        ...envs,
        { ...source!, name: `${String(args.name)} copy` },
      ]);
      return project.info;
    }
    return null;
  });
});

function draftNamed(name: string) {
  const d = environmentsEditor().drafts.find((x) => x.draft.name === name);
  if (!d) throw new Error(`no draft called ${name}`);
  return d;
}

function edit(name: string, baseUrl: string): void {
  const variable = draftNamed(name).draft.variables?.[0];
  if (variable) variable.value = baseUrl;
  flushSync();
}

test("duplicating keeps unsaved edits in every environment", async () => {
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
  edit("local", "http://localhost:9000");
  edit("staging", "https://mine.example.com");
  environmentsEditor().newEnvironment();
  flushSync();

  await environmentsEditor().duplicate(draftNamed("staging"));

  expect(environmentsEditor().drafts.map((d) => d.draft.name)).toEqual([
    "local",
    "staging",
    "staging copy",
    "new environment",
  ]);
  expect(draftNamed("local").draft.variables?.[0]?.value).toBe(
    "http://localhost:9000",
  );
  expect(draftNamed("staging").draft.variables?.[0]?.value).toBe(
    "https://mine.example.com",
  );
  expect(draftNamed("staging").dirty).toBe(true);
});

test("a save that fails partway keeps the drafts it did not write", async () => {
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
  edit("local", "http://localhost:9000");
  draftNamed("staging").setName("qa");
  flushSync();
  taken = ["qa"];

  await expect(environmentsEditor().save()).rejects.toMatchObject({
    kind: "nameTaken",
  });

  expect(saves.map((s) => s.environment.name)).toEqual(["local"]);
  expect(draftNamed("local").dirty).toBe(false);
  // The refused rename is still there to fix.
  expect(draftNamed("qa").dirty).toBe(true);
  expect(environmentsEditor().error).toBe(
    'An environment called "qa" already exists',
  );
});

test("Save refuses, rather than skips, while the editor cannot save", async () => {
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
  // Nothing to save is not a refusal.
  await environmentsEditor().save();
  draftNamed("local").addVariable();
  flushSync();
  // The new row has no name, which is a problem.
  await expect(environmentsEditor().save()).rejects.toThrow(
    "Fix the problems in the environments before saving",
  );
  expect(saves).toEqual([]);
});

test("a new editor tab starts from the files, not old drafts", () => {
  const before = environmentsEditor();
  edit("local", "http://localhost:9000");
  editEnvironments();
  expect(opened).toEqual(["project.environments"]);
  expect(environmentsEditor()).not.toBe(before);
  expect(draftNamed("local").dirty).toBe(false);
});

test("renaming the window's environment keeps the window on it", async () => {
  await project.setEnvironment("staging");
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
  draftNamed("staging").setName("stage");
  flushSync();
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(project.chosen.at(-1)).toBe("stage");
  expect(
    within(screen.getByRole("navigation", { name: "Environments" })).getByRole(
      "button",
      { name: /^stage/ },
    ),
  ).toBeInTheDocument();
});

test("the first variable of an environment without any can be saved", async () => {
  project.info = projectWith([
    env("local", "http://localhost:8080"),
    { name: "staging", production: false },
  ]);
  resetEnvironmentsEditor();
  // Leaving the module and coming back mounts the tab again over the same
  // drafts.
  render(EnvironmentsTab, { tab: { id: "env-tab" } }).unmount();
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
  await userEvent.click(screen.getByRole("button", { name: /^staging/ }));
  await userEvent.click(screen.getByRole("button", { name: "Add variable" }));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Name of variable 1" }),
    "baseUrl",
  );
  await userEvent.type(
    screen.getByRole("textbox", { name: "Value of baseUrl" }),
    "https://staging.example.com",
  );
  const row = screen.getByRole("button", { name: /^staging/ });
  expect(
    within(row).getByRole("img", { name: "Unsaved changes" }),
  ).toBeTruthy();
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(saves.map((s) => s.environment)).toEqual([
    {
      name: "staging",
      production: false,
      variables: [{ name: "baseUrl", value: "https://staging.example.com" }],
    },
  ]);
});
