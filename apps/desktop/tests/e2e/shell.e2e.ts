import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { Key } from "webdriverio";
import {
  MOD,
  controlsOutsideTheWindow,
  lastMeasure,
  mainHeading,
  openProjectAt,
  openTestTabs,
  projectName,
  waitForShell,
  writeProject,
} from "./helpers.js";
import { withWindowSize } from "./requests-support.js";

const MAIN = [
  ["requests", "Requests"],
  ["database", "Database"],
  ["terminal", "Terminal"],
  ["mobile", "Mobile"],
] as const;

describe("the shell", () => {
  const work = mkdtempSync(join(tmpdir(), "dea-e2e-shell-"));

  // Welcome has no rail: the shell shows once a project is open.
  before(async () => {
    await openProjectAt(writeProject("shell-api", work));
  });
  after(() => rmSync(work, { recursive: true, force: true }));

  it("opens each module from the rail and with Mod+1 to Mod+4", async () => {
    // WebKitWebDriver reads title bar text as empty although it is drawn
    // (seen in a CI screenshot), so the helper reads it in the page.
    expect(await projectName()).toBe("shell-api");
    for (const [id, title] of [
      ...MAIN,
      ["vault", "Vault is locked"],
      ["settings", "Appearance"],
    ]) {
      await $(`button[data-module="${id}"]`).click();
      // The Vault has no h1 while locked: it shows an h2 instead.
      await expect(
        id === "vault" ? $("main").$(`h2=${title}`) : mainHeading(title),
      ).toBeDisplayed();
      const ms = await lastMeasure(`module-open:${id}`);
      expect(ms).toBeGreaterThanOrEqual(0);
      expect(ms).toBeLessThanOrEqual(100); // F1-FON-01 #2
    }
    for (const [i, [, title]] of MAIN.entries()) {
      await browser.keys([MOD, String(i + 1)]);
      await expect(mainHeading(title)).toBeDisplayed();
    }
  });

  it("keeps every shell control inside the window at 1024 × 640", async () => {
    await withWindowSize(1024, 640, async () => {
      // The widest shell: Settings with its sidebar, then with the rail open.
      await browser.keys([MOD, ","]);
      await expect(mainHeading("Appearance")).toBeDisplayed();
      // At 1024 px the sidebar steps aside; the title bar brings it back.
      const sidebar = $('aside[aria-label="Settings sidebar"]');
      await expect(sidebar).not.toExist();
      expect(await controlsOutsideTheWindow()).toEqual([]);
      await $('button[aria-label="Show sidebar"]').click();
      await expect(sidebar).toBeDisplayed();
      expect(await controlsOutsideTheWindow()).toEqual([]);
      await $('button[aria-label="Show module names"]').click();
      expect(await controlsOutsideTheWindow()).toEqual([]);
      await $('button[aria-label="Collapse the rail"]').click();
    });
    // Showing it in the narrow window saved nothing: wide again, it was there.
    await expect($('aside[aria-label="Settings sidebar"]')).toBeDisplayed();
  });

  it("keeps theme, sidebar width and shortcuts after a restart", async () => {
    await browser.keys([MOD, ","]);
    await $("button=Light").click();
    await expect($("html")).toHaveAttribute("data-theme", "light");

    const handle = $('[role="separator"]');
    await browser
      .action("pointer")
      .move({ origin: handle })
      .down()
      .move({ origin: "pointer", x: 40, y: 0, duration: 200 })
      .up()
      .perform();
    const width = (await handle.getAttribute("aria-valuenow")) ?? "";
    expect(Number(width)).toBeGreaterThan(280);

    await $("button=Keyboard shortcuts").click();
    const row = $("tr*=Open Settings");
    await row.$("button=Change").click();
    await browser.keys([MOD, Key.Shift, "s"]);
    await row.$("button=Save").click();
    await expect(row).toHaveText(expect.stringContaining("Shift"));

    await browser.pause(800); // the layout is saved 300 ms after the last change
    await browser.reloadSession();
    await waitForShell();

    await expect($("html")).toHaveAttribute("data-theme", "light");
    await expect($('[role="separator"]')).toHaveAttribute(
      "aria-valuenow",
      width,
    );
    await browser.keys([MOD, "1"]);
    await expect(mainHeading("Requests")).toBeDisplayed();
    await browser.keys([MOD, ","]);
    await expect(mainHeading("Requests")).toBeDisplayed();
    await browser.keys([MOD, Key.Shift, "s"]);
    await expect(mainHeading("Appearance")).toBeDisplayed();
  });

  it("switches between 30 tabs in under 50 ms each", async () => {
    await openTestTabs(30);
    await expect(mainHeading("Tab 30")).toBeDisplayed();
    await $$('[role="tab"]')[0]!.click();
    await expect(mainHeading("Tab 1")).toBeDisplayed();
    await browser.execute(() => performance.clearMeasures());
    for (let i = 0; i < 29; i++) await browser.keys(Key.ArrowRight);
    await expect(mainHeading("Tab 30")).toBeDisplayed();
    const durations = await browser.execute(() =>
      performance
        .getEntriesByType("measure")
        .filter((m) => m.name.startsWith("tab-switch:"))
        .map((m) => m.duration),
    );
    expect(durations).toHaveLength(29);
    expect(Math.max(...durations)).toBeLessThan(50); // F1-FON-02 #5
  });
});
