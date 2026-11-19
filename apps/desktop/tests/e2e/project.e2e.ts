// Projects and environments (F1-FON-03 to F1-FON-07). The cases run in
// order and share one app: each starts where the one before stopped.

import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { Key } from "webdriverio";
import {
  MOD,
  appDataDir,
  environmentName,
  lastMeasure,
  openProjectAt,
  pickNextFolder,
  projectName,
  textOf,
  waitForShell,
  waitForText,
  writeProject,
} from "./helpers.js";

const work = mkdtempSync(join(tmpdir(), "dea-e2e-work-"));
const harbor = join(work, "harbor-api");

async function waitForWelcome(): Promise<void> {
  await $("h1=Start a project").waitForDisplayed({ timeout: 60_000 });
}

async function projectMenu(item: string): Promise<void> {
  await $('header button[title="Project"]').click();
  await $('[role="menu"]').$(`button=${item}`).click();
}

async function chooseEnvironment(name: string): Promise<void> {
  await browser.keys([MOD, "e"]);
  await $('[role="menu"]').$(`button*=${name}`).click();
  await browser.waitUntil(async () => (await environmentName()) === name);
}

describe("projects and environments", () => {
  after(() => rmSync(work, { recursive: true, force: true }));

  it("starts empty, opens a folder without dea.yaml and lists recent projects", async () => {
    await waitForWelcome();
    await $("button*=Start empty").click();
    await $('input[aria-label="Project name"]').setValue("harbor-api");
    await $('input[aria-label="Location"]').setValue(harbor);
    await $("button=Create project").click();
    await waitForShell();
    expect(await projectName()).toBe("harbor-api");

    // Dea opens the last project again.
    await browser.reloadSession();
    await waitForShell();
    expect(await projectName()).toBe("harbor-api");

    // A second project, whose folder goes away later.
    const gone = writeProject("gone-api", work);
    await openProjectAt(gone);
    await projectMenu("Close project");
    await waitForWelcome();
    const list = "ul[aria-labelledby='recent-label']";
    const recent = () => textOf(list);
    // The list loads after Welcome shows.
    await waitForText(list, "gone-api");
    expect(await recent()).toContain("harbor-api");

    // A folder without dea.yaml.
    const plain = join(work, "plain");
    mkdirSync(plain);
    pickNextFolder(plain);
    await $("button*=Open a project folder").click();
    await waitForText("[role='alert']", "so it is not a Dea project");

    // A recent project whose folder is gone.
    rmSync(gone, { recursive: true, force: true });
    await browser.reloadSession();
    await waitForWelcome();
    // Only a recent project whose folder is gone can be removed here.
    await $("li*=gone-api").$("button=Remove from list").click();
    await browser.waitUntil(
      async () => !(await recent())?.includes("gone-api"),
    );
    expect(await recent()).toContain("harbor-api");
  });

  it("switches the environment in under 200 ms and marks production", async () => {
    await $("li*=harbor-api").$("button").click();
    await waitForShell();
    expect(await environmentName()).toBe("local");

    await chooseEnvironment("production");
    const ms = await lastMeasure("env-switch");
    expect(ms).toBeGreaterThanOrEqual(0);
    expect(ms).toBeLessThanOrEqual(200); // F1-FON-06 #5
    expect(
      await textOf('header button[title="Environment for requests"]'),
    ).toContain("production");
    await expect($("footer .env-prod-status")).toBeExisting();
  });

  it("opens the same project in a second window with another environment", async () => {
    const first = await browser.getWindowHandle();
    const openSecond = async (): Promise<string> => {
      await browser.keys([MOD, Key.Shift, "n"]);
      await browser.waitUntil(
        async () => (await browser.getWindowHandles()).length === 2,
        { timeout: 30_000, timeoutMsg: "no second window" },
      );
      const handle = (await browser.getWindowHandles()).find(
        (h) => h !== first,
      )!;
      await browser.switchToWindow(handle);
      await waitForShell();
      return handle;
    };

    await openSecond();
    expect(await projectName()).toBe("harbor-api");
    expect(await environmentName()).toBe("local");
    await chooseEnvironment("staging");
    await browser.switchToWindow(first);
    expect(await environmentName()).toBe("production");

    // Each window of the project keeps its own choice across a restart.
    await browser.pause(800); // the layout is saved 300 ms after a change
    await browser.reloadSession();
    await waitForShell();
    expect(await environmentName()).toBe("production");
    const restarted = await browser.getWindowHandle();
    await browser.keys([MOD, Key.Shift, "n"]);
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 2,
      { timeout: 30_000, timeoutMsg: "no second window after a restart" },
    );
    const second = (await browser.getWindowHandles()).find(
      (h) => h !== restarted,
    )!;
    await browser.switchToWindow(second);
    await waitForShell();
    expect(await environmentName()).toBe("staging");
    await browser.closeWindow();
    await browser.switchToWindow(restarted);
  });

  it("reloads an environment edited outside Dea within a second", async () => {
    await browser.keys([MOD, "e"]);
    await $('[role="menu"]').$("button*=Edit environments").click();
    const list = $("nav[aria-label='Environments']");
    await list.$("button*=staging").click();
    await expect($("main").$("h1=staging")).toBeDisplayed();

    writeFileSync(
      join(harbor, "environments", "staging.yaml"),
      "name: staging\nproduction: false\nvariables:\n  - name: FROM_DISK\n    value: written outside\n",
    );
    const value = $("input[aria-label='Value of FROM_DISK']");
    await value.waitForExist({ timeout: 1_000 }); // F1-FON-05 #1
    expect(await value.getValue()).toBe("written outside");
  });

  it("clears local data", async () => {
    // The open Environments tab is in the saved layout.
    await browser.pause(800);
    const id = /^id: (.+)$/m.exec(
      readFileSync(join(harbor, "dea.yaml"), "utf8"),
    )?.[1];
    expect(id).toBeTruthy();
    const folder = join(
      appDataDir(),
      "ui",
      createHash("sha256").update(id!).digest("hex").slice(0, 16),
    );
    // Every window slot of the project, with its saved tab count. The
    // window under test is usually slot 0, but after the second-window
    // case it may not be.
    const slots = () =>
      existsSync(folder)
        ? readdirSync(folder)
            .filter((f) => f.endsWith(".json"))
            .map((f) => ({
              file: f,
              tabs: (
                JSON.parse(readFileSync(join(folder, f), "utf8")) as {
                  tabs: unknown[];
                }
              ).tabs.length,
            }))
        : [];
    const before = slots();
    if (!before.some((s) => s.tabs > 0))
      throw new Error(
        `no saved tabs before clearing: ${JSON.stringify(before)}`,
      );

    await projectMenu("Clear local data…");
    await $('[role="dialog"]').$("button=Clear").click();
    await waitForText(".toasts", "Local data cleared");
    await browser.waitUntil(
      async () => (await $$('[role="tab"]').length) === 0,
    );
    await browser.pause(800);
    // Gone, or written again with the empty layout: either way no tabs.
    const after = slots();
    if (after.some((s) => s.tabs > 0))
      throw new Error(`tabs left after clearing: ${JSON.stringify(after)}`);
  });
});
