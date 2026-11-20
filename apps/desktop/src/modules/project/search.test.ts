// Environments in the command palette (F1-FON-11 #2, #6).

import { beforeEach, expect, test } from "vitest";
import { setProjectApi } from "$app/contributions/project-api";
import { setShellApi } from "$app/contributions/shell-api";
import type { Environment } from "$lib/ipc/bindings/Environment";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import { FakeProject } from "./fake-project.svelte";
import { manifest } from "./index";
import environments from "./search";
import { info as baseInfo } from "./test-support";

const local: Environment = { name: "local", production: false };
const production: Environment = { name: "production", production: true };

function projectWith(envs: Environment[]): ProjectInfo {
  return { ...baseInfo("harbor-api", "/code/harbor-api"), environments: envs };
}

let project: FakeProject;
let toasts: string[];

beforeEach(() => {
  project = new FakeProject(projectWith([local, production]));
  setProjectApi(project);
  toasts = [];
  setShellApi({
    openTab: () => "t",
    findTab: () => undefined,
    updateTab: () => {},
    activateTab: () => true,
    closeTab: () => Promise.resolve(true),
    setTabDirty: () => {},
    setSaveHandler: () => {},
    openModule: () => Promise.resolve(),
    runCommand: () => Promise.resolve(),
    toast: (text) => toasts.push(text),
    dismissToast: () => {},
  });
});

test("the project module lists environments in the palette", () => {
  expect(manifest.searchSources).toEqual([
    expect.objectContaining({
      id: "project.searchEnvironments",
      group: "Environments",
      order: 60,
    }),
  ]);
});

test("environments are listed with Use for requests and production marked", async () => {
  const items = environments();
  expect(
    items.map((i) => [i.title, i.meta, i.environment, i.opensTab]),
  ).toEqual([
    [
      "local",
      "Use for requests",
      { name: "local", production: false },
      undefined,
    ],
    [
      "production",
      "Use for requests",
      { name: "production", production: true },
      undefined,
    ],
  ]);
  await items[1]?.open("active");
  expect(project.chosen).toEqual(["production"]);
});

test("an environment that is gone says so", async () => {
  const items = environments();
  project.info = projectWith([local]);
  await items[1]?.open("active");
  expect(toasts).toEqual(["production no longer exists"]);
  expect(project.chosen).toEqual([]);
});

test("Welcome has no environments to list", () => {
  project.info = null;
  expect(environments()).toEqual([]);
});

test("environment commands need a project", () => {
  const scoped = (manifest.commands ?? [])
    .filter((c) => c.scope === "project")
    .map((c) => c.id);
  expect(scoped).toEqual([
    "project.editEnvironments",
    "project.saveEnvironments",
  ]);
});
