// The command palette (F1-FON-11): Mod+K from every module, the first
// result in time, and opening what it finds.

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { Key } from "webdriverio";
import {
  environmentName,
  lastMeasure,
  mainHeading,
  openPalette,
  openProjectAt,
  paletteActive,
  paletteSearch,
  waitForNoDialog,
  waitForText,
  writeProject,
} from "./helpers.js";

const MODULES = [
  "requests",
  "database",
  "terminal",
  "mobile",
  "vault",
  "settings",
];

describe("the command palette", () => {
  const work = mkdtempSync(join(tmpdir(), "dea-e2e-palette-"));

  before(async () => {
    const root = writeProject("palette-api", work);
    // A declared secret: the palette lists its name without a value.
    writeFileSync(
      join(root, "vault.yaml"),
      "secrets:\n  - name: access_token\n    type: token\n",
    );
    await openProjectAt(root);
  });
  after(() => rmSync(work, { recursive: true, force: true }));

  it("opens with Mod+K from every module and from the Search box", async () => {
    for (const id of MODULES) {
      await $(`button[data-module="${id}"]`).click();
      await openPalette();
      await browser.keys(Key.Escape);
      await waitForNoDialog();
    }
    await $("header").$("button*=Search").click();
    await $('[role="dialog"]').$('input[role="combobox"]').waitForDisplayed();
    await browser.keys(Key.Escape);
    await waitForNoDialog();
  });

  it("shows the first result within 100 ms", async () => {
    await browser.execute(() => performance.clearMeasures());
    await openPalette();
    await browser.waitUntil(async () => (await paletteActive()) !== null);
    const ms = await lastMeasure("palette-first-result");
    expect(ms).toBeGreaterThanOrEqual(0);
    expect(ms).toBeLessThanOrEqual(100); // F1-FON-11 #4
    await browser.keys(Key.Escape);
    await waitForNoDialog();
  });

  it("switches the environment from the palette", async () => {
    expect(await environmentName()).toBe("local");
    await openPalette();
    await paletteSearch("production");
    await browser.waitUntil(
      async () => (await paletteActive()) === "production",
    );
    await browser.keys(Key.Enter);
    await waitForNoDialog();
    await browser.waitUntil(
      async () => (await environmentName()) === "production",
    );
  });

  it("runs a command and opens a Settings section", async () => {
    await openPalette();
    await paletteSearch("keyboard shortcuts");
    await browser.waitUntil(
      async () => (await paletteActive()) === "Keyboard shortcuts",
    );
    await browser.keys(Key.Enter);
    await expect(mainHeading("Keyboard shortcuts")).toBeDisplayed();

    await openPalette();
    await paletteSearch("open vault");
    await browser.waitUntil(
      async () => (await paletteActive()) === "Open Vault",
    );
    await browser.keys(Key.Enter);
    await expect($("main").$("h2=Vault is locked")).toBeDisplayed();
  });

  it("finds a secret name without showing its value", async () => {
    await openPalette();
    await paletteSearch("access");
    await browser.waitUntil(
      async () => (await paletteActive()) === "access_token",
    );
    await waitForText('[role="listbox"]', "Name only, values never show here");
    await browser.keys(Key.Enter);
    await waitForNoDialog();
    // The Vault opens on it; locked, it shows the locked screen.
    await expect($("main").$("h2=Vault is locked")).toBeDisplayed();
  });
});
