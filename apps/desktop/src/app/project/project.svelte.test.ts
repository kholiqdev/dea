import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushSync } from "svelte";
import { beforeEach, expect, test } from "vitest";
import type { ProjectChanged } from "$lib/ipc/bindings/ProjectChanged";
import { projectInfo } from "../shell/test-support";
import { WindowProject, activeEnvironment } from "./project.svelte";

let calls: { cmd: string; args: unknown }[];
let info = projectInfo();

beforeEach(() => {
  calls = [];
  info = projectInfo();
  clearMocks();
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "window_project") return info;
    return null;
  });
});

test("a_missing_active_environment_falls_back", () => {
  expect(activeEnvironment(info, "staging")?.name).toBe("staging");
  expect(activeEnvironment(info, "qa")?.name).toBe("local");
  expect(
    activeEnvironment({ ...info, defaultEnvironment: null }, "qa")?.name,
  ).toBe("local");
  expect(
    activeEnvironment({ ...info, defaultEnvironment: "gone" }, null)?.name,
  ).toBe("local");
  expect(activeEnvironment({ ...info, environments: [] }, "qa")).toBeNull();
  expect(activeEnvironment(null, "qa")).toBeNull();
});

test("load reads this window's project", async () => {
  const p = new WindowProject();
  expect(await p.load()).toEqual(info);
  expect(p.info?.name).toBe("harbor-api");
  expect(p.environment?.name).toBe("local");
});

test("switching the environment tells Dea and changes the active one", async () => {
  const p = new WindowProject();
  await p.load();
  await p.setEnvironment("production");
  expect(p.environment?.production).toBe(true);
  expect(calls).toContainEqual({
    cmd: "set_active_environment",
    args: { name: "production" },
  });
});

test("a change from disk replaces the project and lists the files", async () => {
  const p = new WindowProject();
  await p.load();
  p.restoreEnvironment("staging");
  const change: ProjectChanged = {
    info: projectInfo({ environments: [{ name: "local", production: false }] }),
    changed: ["environments/staging.yaml"],
  };
  p.apply(change);
  flushSync();
  expect(p.changed).toEqual(["environments/staging.yaml"]);
  // staging is gone: back to the default environment.
  expect(p.environment?.name).toBe("local");
});

test("a dea.yaml that cannot be read keeps what the window showed", async () => {
  const p = new WindowProject();
  await p.load();
  p.restoreEnvironment("staging");
  const broken = projectInfo({
    environments: [],
    environmentFiles: {},
    readOnly: true,
    errors: [
      {
        path: "dea.yaml",
        line: null,
        message:
          "unresolved merge conflict: resolve the conflict markers and save",
        mergeConflict: true,
      },
    ],
  });
  p.apply({ info: broken, changed: ["dea.yaml"] });
  flushSync();
  expect(p.info?.environments.map((e) => e.name)).toEqual([
    "local",
    "staging",
    "production",
  ]);
  expect(p.environment?.name).toBe("staging");
  expect(p.info?.errors).toEqual(broken.errors);

  // Fixed: the next change carries the whole project again.
  p.apply({ info: projectInfo(), changed: ["dea.yaml"] });
  expect(p.info?.errors).toEqual([]);
});

test("a window that gains focus reads its project again", async () => {
  const p = new WindowProject();
  await p.load();
  const stop = p.recheckOnFocus();
  info = projectInfo({ missing: true, environments: [] });
  window.dispatchEvent(new Event("focus"));
  await expect.poll(() => p.info?.missing).toBe(true);
  stop();
  // A failed read keeps what the window has.
  clearMocks();
  mockIPC(() => {
    throw new Error("gone");
  });
  window.dispatchEvent(new Event("focus"));
  await new Promise((r) => setTimeout(r, 0));
  expect(p.info?.missing).toBe(true);
});
