// Project files changed outside Dea while the environment editor is open
// (F1-FON-05).

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { FileErrorInfo } from "$lib/ipc/bindings/FileErrorInfo";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import EnvironmentsTab from "./EnvironmentsTab.svelte";
import WatchingStatus from "./components/WatchingStatus.svelte";
import { FakeProject } from "./fake-project.svelte";
import {
  environmentsEditor,
  resetEnvironmentsEditor,
} from "./state/environments.svelte";
import { info as baseInfo } from "./test-support";

function env(name: string, baseUrl: string, production = false): Environment {
  return { name, production, variables: [{ name: "baseUrl", value: baseUrl }] };
}

const local = env("local", "http://localhost:8080");
const staging = env("staging", "https://staging.example.com");
const production = env("production", "https://example.com", true);

function projectWith(
  environments: Environment[],
  extra: Partial<ProjectInfo> = {},
): ProjectInfo {
  return {
    ...baseInfo("harbor-api", "/code/harbor-api"),
    environments,
    environmentFiles: Object.fromEntries(
      environments.map((e) => [e.name, `environments/${e.name}.yaml`]),
    ),
    ...extra,
  };
}

let project: FakeProject;
let saves: { previous: string | null; environment: Environment }[];

function useProject(info: ProjectInfo): void {
  project = new FakeProject(info);
  setProjectApi(project);
}

beforeEach(() => {
  resetEnvironmentsEditor();
  useProject(projectWith([local, staging, production]));
  saves = [];
  setShellApi({
    openTab: () => "t",
    findTab: () => undefined,
    updateTab: () => {},
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    toast: () => 0,
    dismissToast: () => {},
    runCommand: () => Promise.resolve(),
  });
  clearMocks();
  mockIPC((cmd, raw) => {
    const args = raw as Record<string, unknown>;
    if (cmd !== "save_environment") return null;
    const environment = args.environment as Environment;
    const previous = args.previous as string | null;
    saves.push({ previous, environment });
    return projectWith([
      ...(project.info?.environments ?? []).filter(
        (e) => e.name !== previous && e.name !== environment.name,
      ),
      environment,
    ]);
  });
});

function open(): void {
  render(EnvironmentsTab, { tab: { id: "env-tab" } });
}

async function select(name: string): Promise<void> {
  await userEvent.click(
    within(screen.getByRole("navigation", { name: "Environments" })).getByRole(
      "button",
      { name: new RegExp(`^${name}`) },
    ),
  );
}

function draftNamed(name: string) {
  const d = environmentsEditor().drafts.find((x) => x.draft.name === name);
  if (!d) throw new Error(`no draft called ${name}`);
  return d;
}

/** Edits the base URL of an environment, as typing in its row would. */
function edit(name: string, baseUrl: string): void {
  const variable = draftNamed(name).draft.variables?.[0];
  if (variable) variable.value = baseUrl;
  flushSync();
}

/** One watcher batch: the project as it is now, and the files it touched. */
function changeOnDisk(info: ProjectInfo, changed: string[]): void {
  project.apply({ info, changed });
  flushSync();
}

function banners(): HTMLElement[] {
  return screen.queryAllByRole("status");
}

test("only_the_edited_environment_conflicts", async () => {
  open();
  await select("staging");
  edit("staging", "https://mine.example.com");

  changeOnDisk(
    projectWith([
      local,
      env("staging", "https://theirs.example.com"),
      env("production", "https://new.example.com", true),
    ]),
    ["environments/staging.yaml", "environments/production.yaml"],
  );

  expect(banners()).toHaveLength(1);
  expect(banners()[0]).toHaveTextContent(
    "environments/staging.yaml changed on disk while you were editing it.",
  );
  expect(draftNamed("staging").draft.variables?.[0]?.value).toBe(
    "https://mine.example.com",
  );
  // An environment without local changes takes the disk version silently.
  expect(draftNamed("production").draft.variables?.[0]?.value).toBe(
    "https://new.example.com",
  );
  expect(draftNamed("production").dirty).toBe(false);
  await select("production");
  expect(banners()).toHaveLength(0);
});

test("Keep mine keeps the draft, Take theirs loads the disk version", async () => {
  open();
  await select("staging");
  edit("staging", "https://mine.example.com");
  changeOnDisk(
    projectWith([
      local,
      env("staging", "https://theirs.example.com"),
      production,
    ]),
    ["environments/staging.yaml"],
  );

  await userEvent.click(screen.getByRole("button", { name: "Compare" }));
  const compare = screen.getByRole("region", { name: "Compare" });
  expect(
    within(compare).getByRole("group", { name: "Mine, not saved" }),
  ).toHaveTextContent("baseUrl: https://mine.example.com");
  expect(
    within(compare).getByRole("group", { name: "On disk" }),
  ).toHaveTextContent("baseUrl: https://theirs.example.com");

  await userEvent.click(screen.getByRole("button", { name: "Keep mine" }));
  expect(banners()).toHaveLength(0);
  expect(screen.queryByRole("region", { name: "Compare" })).toBeNull();
  expect(draftNamed("staging").dirty).toBe(true);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(saves.at(-1)).toEqual({
    previous: "staging",
    environment: expect.objectContaining({
      variables: [{ name: "baseUrl", value: "https://mine.example.com" }],
    }) as Environment,
  });

  edit("staging", "https://mine-again.example.com");
  changeOnDisk(
    projectWith([
      local,
      env("staging", "https://theirs-again.example.com"),
      production,
    ]),
    ["environments/staging.yaml"],
  );
  await userEvent.click(screen.getByRole("button", { name: "Take theirs" }));
  expect(banners()).toHaveLength(0);
  expect(draftNamed("staging").draft.variables?.[0]?.value).toBe(
    "https://theirs-again.example.com",
  );
  expect(draftNamed("staging").dirty).toBe(false);
});

test("a deleted environment file offers Save it again and Discard changes", async () => {
  open();
  await select("staging");
  edit("staging", "https://mine.example.com");
  edit("local", "http://localhost:9090");

  changeOnDisk(projectWith([production]), [
    "environments/staging.yaml",
    "environments/local.yaml",
  ]);
  expect(banners()).toHaveLength(1);
  expect(banners()[0]).toHaveTextContent(
    "environments/staging.yaml was deleted outside Dea. Your changes to it are still here.",
  );

  await userEvent.click(screen.getByRole("button", { name: "Save it again" }));
  expect(banners()).toHaveLength(0);
  await select("local");
  await userEvent.click(
    within(banners()[0]!).getByRole("button", { name: "Discard changes" }),
  );
  expect(environmentsEditor().drafts.map((d) => d.draft.name)).toEqual([
    "production",
    "staging",
  ]);

  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(saves).toEqual([
    {
      previous: null,
      environment: expect.objectContaining({ name: "staging" }) as Environment,
    },
  ]);
});

test("an environment deleted outside Dea without local changes goes away", () => {
  open();
  changeOnDisk(projectWith([local, production]), ["environments/staging.yaml"]);
  expect(banners()).toHaveLength(0);
  expect(environmentsEditor().drafts.map((d) => d.draft.name)).toEqual([
    "local",
    "production",
  ]);
});

const conflict: FileErrorInfo = {
  path: "environments/staging.yaml",
  line: null,
  message: "unresolved merge conflict: resolve the conflict markers and save",
  mergeConflict: true,
};

test("merge conflict markers block saving that environment", async () => {
  open();
  await select("staging");
  edit("staging", "https://mine.example.com");

  changeOnDisk(projectWith([local, production], { errors: [conflict] }), [
    "environments/staging.yaml",
  ]);
  expect(
    screen.getByRole("heading", { name: "Merge conflicts" }),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/has Git conflict markers/).closest("p"),
  ).toHaveTextContent(
    "environments/staging.yaml has Git conflict markers. Resolve them in your editor; Dea reloads the file when it is clean.",
  );
  expect(draftNamed("staging").dirty).toBe(true);
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  await expect(environmentsEditor().save()).rejects.toThrow(
    "Decide what to do with the changes on disk before saving",
  );
  expect(saves).toHaveLength(0);

  // Resolved in the editor: the file loads again.
  changeOnDisk(projectWith([local, staging, production]), [
    "environments/staging.yaml",
  ]);
  expect(screen.queryByRole("heading", { name: "Merge conflicts" })).toBeNull();
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

test("a file that cannot be read names its line", () => {
  open();
  changeOnDisk(
    projectWith([local, production], {
      errors: [
        {
          path: "environments/staging.yaml",
          line: 14,
          message:
            "mapping values are not allowed in this context at line 14 column 9",
          mergeConflict: false,
        },
      ],
    }),
    ["environments/staging.yaml"],
  );
  expect(
    screen.getByRole("heading", { name: "A file that cannot be read" }),
  ).toBeInTheDocument();
  expect(screen.getByText(/line 14:/).closest("p")).toHaveTextContent(
    "environments/staging.yaml, line 14: mapping values are not allowed in this context. The environment is not loaded until the file is fixed.",
  );
});

test("a read-only project disables every input", () => {
  useProject(projectWith([local, staging], { readOnly: true }));
  open();
  expect(banners()[0]).toHaveTextContent(
    "This project was saved by a newer Dea. It is open read-only so nothing is lost.",
  );
  const inputs = [...document.querySelectorAll("input")];
  expect(inputs.length).toBeGreaterThan(0);
  for (const input of inputs)
    expect(input.readOnly || input.disabled).toBe(true);
  for (const radio of screen.getAllByRole("radio"))
    expect(radio).toBeDisabled();
  expect(screen.getByRole("switch")).toBeDisabled();
  for (const name of [
    "Save",
    "New environment",
    "Add variable",
    "Duplicate",
    "Rename",
  ])
    expect(screen.queryByRole("button", { name })).toBeNull();
});

test("500 changed files apply as one update within 200 ms", () => {
  const many = Array.from({ length: 50 }, (_, i) => ({
    name: `env-${i}`,
    production: false,
    variables: Array.from({ length: 10 }, (_, j) => ({
      name: `V${j}`,
      value: `v${i}-${j}`,
    })),
  }));
  useProject(projectWith(many));
  open();
  const sync = vi.spyOn(environmentsEditor(), "sync");
  const changed = many.map((e) => ({
    ...e,
    variables: e.variables.map((v) => ({ ...v, value: `${v.value}!` })),
  }));

  const start = performance.now();
  changeOnDisk(
    projectWith(changed),
    Array.from({ length: 500 }, (_, i) => `environments/file-${i}.yaml`),
  );
  const elapsed = performance.now() - start;

  expect(sync).toHaveBeenCalledOnce();
  expect(elapsed).toBeLessThan(200);
  expect(environmentsEditor().drafts[0]?.draft.variables?.[0]?.value).toBe(
    "v0-0!",
  );
});

test("the status bar says project files are watched", () => {
  const { unmount } = render(WatchingStatus);
  expect(
    screen.getByText("Project files are watched for changes"),
  ).toBeInTheDocument();
  unmount();
  useProject(projectWith([], { missing: true }));
  render(WatchingStatus);
  expect(
    screen.queryByText("Project files are watched for changes"),
  ).toBeNull();
});

test("a vault.yaml that cannot be read is listed too", () => {
  useProject(
    projectWith([local], {
      errors: [
        {
          path: "vault.yaml",
          line: 3,
          message: "did not find expected key",
          mergeConflict: false,
        },
      ],
    }),
  );
  open();
  expect(screen.getByText(/line 3:/).closest("p")).toHaveTextContent(
    "vault.yaml, line 3: did not find expected key. Dea skips the file until it is fixed.",
  );
});

test("a dea.yaml that cannot be read is not called a newer Dea", () => {
  useProject(
    projectWith([], {
      readOnly: true,
      errors: [
        {
          path: "dea.yaml",
          line: null,
          message: "unresolved merge conflict",
          mergeConflict: true,
        },
      ],
    }),
  );
  open();
  expect(screen.queryByText(/saved by a newer Dea/)).toBeNull();
  // The window's banner names dea.yaml; the editor does not repeat it.
  expect(screen.queryByText(/has Git conflict markers/)).toBeNull();
});
