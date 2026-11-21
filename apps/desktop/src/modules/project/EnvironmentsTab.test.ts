import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { setProjectApi, type ProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import EnvironmentsTab from "./EnvironmentsTab.svelte";
import { resetEnvironmentsEditor } from "./state/environments.svelte";
import { info as baseInfo } from "./test-support";

let project: ProjectInfo;
let saved: { previous: string | null; environment: Environment }[];
let dirty: Record<string, boolean>;
let refreshed: ProjectInfo[];

function projectWith(
  environments: Environment[],
  secretNames: string[] = [],
): ProjectInfo {
  return {
    ...baseInfo("harbor-api", "/code/harbor-api"),
    environments,
    secretNames,
  };
}

beforeEach(() => {
  project = projectWith(
    [
      {
        name: "local",
        production: false,
        variables: [{ name: "baseUrl", value: "http://localhost:8080" }],
      },
      { name: "staging", production: false, color: "blue" },
      { name: "production", production: true },
    ],
    ["access_token"],
  );
  saved = [];
  dirty = {};
  refreshed = [];
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
      refreshed.push(info);
    },
  };
  setProjectApi(api);
  setShellApi({
    openTab: () => "t",
    findTab: () => undefined,
    updateTab: () => {},
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: (id, d) => (dirty[id] = d),
    activeTab: () => null,
    isTabDirty: (id) => dirty[id] ?? false,
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    toast: () => 0,
    dismissToast: () => {},
    runCommand: () => Promise.resolve(),
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = raw as Record<string, unknown>;
    if (cmd === "save_environment") {
      const env = args.environment as Environment;
      if (env.name === "local-taken")
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw {
          kind: "nameTaken",
          message: 'an environment called "local-taken" already exists',
        };
      saved.push({
        previous: args.previous as string | null,
        environment: env,
      });
      const prev = args.previous as string | null;
      project = {
        ...project,
        environments: [
          ...project.environments.filter(
            (e) => e.name !== prev && e.name !== env.name,
          ),
          env,
        ],
      };
      return project;
    }
    if (cmd === "environment_references")
      return [{ label: "Default environment of the project", count: 1 }];
    if (cmd === "delete_environment") {
      project = {
        ...project,
        environments: project.environments.filter((e) => e.name !== args.name),
      };
      return project;
    }
    return null;
  });
});

function open() {
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
}

async function select(name: string) {
  await userEvent.click(
    within(screen.getByRole("navigation", { name: "Environments" })).getByRole(
      "button",
      { name: new RegExp(`^${name}`) },
    ),
  );
}

test("editing a variable marks the tab dirty and Save writes only changed environments", async () => {
  open();
  await select("local");
  const value = screen.getByRole("textbox", { name: "Value of baseUrl" });
  await userEvent.clear(value);
  await userEvent.type(value, "http://localhost:9000");
  await waitFor(() => expect(dirty["env-tab"]).toBe(true));
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(saved).toHaveLength(1);
  expect(saved[0]?.previous).toBe("local");
  expect(saved[0]?.environment.variables?.[0]?.value).toBe(
    "http://localhost:9000",
  );
  await waitFor(() => expect(dirty["env-tab"]).toBe(false));
  expect(refreshed).toHaveLength(1);
});

test("problems are shown before saving", async () => {
  open();
  await select("local");
  await userEvent.click(screen.getByRole("button", { name: "Add variable" }));
  await userEvent.type(
    screen.getAllByRole("textbox", { name: /^Name of variable/ }).at(-1)!,
    "baseUrl",
  );
  expect(screen.getByRole("alert")).toHaveTextContent(
    'The variable "baseUrl" is listed twice',
  );
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
});

test("a Vault reference to an unknown secret warns but saves", async () => {
  open();
  await select("staging");
  await userEvent.click(screen.getByRole("button", { name: "Add variable" }));
  const names = screen.getAllByRole("textbox", { name: /^Name of variable/ });
  await userEvent.type(names.at(-1)!, "token");
  await userEvent.type(
    screen.getByRole("textbox", { name: "Value of token" }),
    "{{{{vault:missing}}",
  );
  expect(
    screen.getByText("Not in this project's Vault yet"),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(saved).toHaveLength(1);
});

test("deleting lists what uses the environment first", async () => {
  open();
  await select("staging");
  await userEvent.click(
    screen.getByRole("button", { name: "Delete staging…" }),
  );
  const dialog = await screen.findByRole("dialog", { name: "Delete staging?" });
  expect(
    await within(dialog).findByText("Default environment of the project"),
  ).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  await userEvent.click(
    within(dialog).getByRole("button", { name: "Delete staging" }),
  );
  await waitFor(() =>
    expect(project.environments.map((e) => e.name)).toEqual([
      "local",
      "production",
    ]),
  );
});

test("production can be turned on without signing in", async () => {
  open();
  await select("production");
  expect(
    screen.getByRole("switch", { name: "Treat as production" }),
  ).toBeEnabled();
  await select("staging");
  const stagingSwitch = screen.getByRole("switch", {
    name: "Treat as production",
  });
  await userEvent.click(stagingSwitch);
  expect(stagingSwitch).toHaveAttribute("aria-checked", "true");
});

test("renaming to a taken name shows the error from Dea", async () => {
  open();
  await select("local");
  const name = screen.getByRole("textbox", { name: "Name" });
  await userEvent.clear(name);
  await userEvent.type(name, "local-taken");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(
    await screen.findByText(
      'An environment called "local-taken" already exists',
    ),
  ).toBeInTheDocument();
});

test("a new environment named prod, production or prd starts as production", async () => {
  open();
  await userEvent.click(
    screen.getByRole("button", { name: "New environment" }),
  );
  const name = screen.getByRole("textbox", { name: "Name" });
  await userEvent.clear(name);
  await userEvent.type(name, "prd");
  expect(
    screen.getByRole("switch", { name: "Treat as production" }),
  ).toHaveAttribute("aria-checked", "true");
});

test("the variable count reads in the singular for one variable", async () => {
  open();
  await select("local");
  expect(screen.getByText(/^1 variable\. /)).toBeInTheDocument();
  await select("staging");
  expect(screen.getByText(/^0 variables\. /)).toBeInTheDocument();
});
