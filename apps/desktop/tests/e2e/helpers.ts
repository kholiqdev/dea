import { mkdirSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { $, browser } from "@wdio/globals";
import { Key, type ChainablePromiseElement } from "webdriverio";

/** Mod is Ctrl on Linux and Windows, the only places these tests run. */
export const MOD = Key.Ctrl;

export async function waitForShell(): Promise<void> {
  await $('nav[aria-label="Modules"]').waitForDisplayed({ timeout: 60_000 });
}

/** The heading of the view in the work area. */
export function mainHeading(text: string) {
  return $("main").$(`h1=${text}`);
}

/** How long the last `name` measure took, in ms, or -1 if none. */
export function lastMeasure(name: string): Promise<number> {
  return browser.execute(
    (n) => performance.getEntriesByName(n, "measure").at(-1)?.duration ?? -1,
    name,
  );
}

/** Shell controls and status items that are not fully inside the window. */
export function controlsOutsideTheWindow(): Promise<string[]> {
  return browser.execute(() => {
    const w = window.innerWidth;
    const h = window.innerHeight;
    return [
      ...document.querySelectorAll("[data-shell-control], [data-status-item]"),
    ]
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return (
          r.width === 0 ||
          r.left < 0 ||
          r.top < 0 ||
          r.right > w ||
          r.bottom > h
        );
      })
      .map((el) => el.outerHTML.slice(0, 100));
  });
}

/** Opens `count` test tabs through the debug-only hook. */
export function openTestTabs(count: number): Promise<void> {
  return browser.execute((n) => {
    const api = (
      window as unknown as { __deaE2e: { openTab(t: string): string } }
    ).__deaE2e;
    for (let i = 1; i <= n; i++) api.openTab(`Tab ${i}`);
  }, count);
}

/** Text of the first element matching `selector`, read in the page. */
export function textOf(selector: string): Promise<string | null> {
  return browser.execute(
    (s) => document.querySelector(s)?.textContent?.trim() ?? null,
    selector,
  );
}

/** The project name in the title bar. */
export function projectName(): Promise<string | null> {
  return textOf("header .where .name");
}

/** The environment in the title bar. */
export function environmentName(): Promise<string | null> {
  return textOf('header button[title="Environment for requests"] .name');
}

/** The app data folder of this run (see wdio.conf.ts). */
export function appDataDir(): string {
  const dir = process.env.DEA_APP_DATA_DIR;
  if (!dir) throw new Error("DEA_APP_DATA_DIR is not set");
  return dir;
}

/** The folder the next "pick a folder" returns, instead of the dialog. */
export function pickNextFolder(path: string): void {
  const file = process.env.DEA_E2E_PICK_FILE;
  if (!file) throw new Error("DEA_E2E_PICK_FILE is not set");
  writeFileSync(file, path);
}

/**
 * The file the next "pick a file" returns, instead of the dialog. The folder
 * and the file picker read the same one-shot file (see wdio.conf.ts).
 */
export function pickNextFile(path: string): void {
  pickNextFolder(path);
}

/** Writes a project with local, staging and production into a new folder. */
export function writeProject(name: string, parent: string): string {
  const root = join(parent, name);
  mkdirSync(join(root, "environments"), { recursive: true });
  writeFileSync(
    join(root, "dea.yaml"),
    `name: ${name}\nid: e2e-${name}\nschemaVersion: 1\ndefaultEnvironment: local\n`,
  );
  for (const [env, production] of [
    ["local", false],
    ["staging", false],
    ["production", true],
  ] as const)
    writeFileSync(
      join(root, "environments", `${env}.yaml`),
      `name: ${env}\nproduction: ${production}\n`,
    );
  return root;
}

/**
 * Opens the project at `root` in this window and waits for the shell. The
 * project is named after its folder, as writeProject names it.
 */
export async function openProjectAt(root: string): Promise<void> {
  await browser.waitUntil(() => browser.execute(() => "__deaE2e" in window), {
    timeout: 60_000,
    timeoutMsg: "the app did not start",
  });
  await browser.execute((r) => {
    const api = (
      window as unknown as { __deaE2e: { openProject(r: string): void } }
    ).__deaE2e;
    api.openProject(r);
  }, root);
  await browser.waitUntil(
    async () => (await projectName()) === basename(root),
    { timeout: 30_000, timeoutMsg: `the project at ${root} did not open` },
  );
  await waitForShell();
}

/**
 * Types `text` into the focused element. `browser.keys` holds every key of
 * its text down before it lets any go, and WebKitWebDriver (Linux) types
 * nothing for a key that is already down: "https://" arrives as "htps:/".
 * So each call gets only keys that are not down yet.
 */
export async function typeKeys(text: string): Promise<void> {
  let keys = "";
  for (const key of text) {
    if (keys.includes(key)) {
      await browser.keys(keys);
      keys = "";
    }
    keys += key;
  }
  if (keys !== "") await browser.keys(keys);
}

/** Waits until the first `selector` match contains `text`. */
export async function waitForText(
  selector: string,
  text: string,
  timeout = 15_000,
): Promise<void> {
  await browser.waitUntil(
    async () => (await textOf(selector))?.includes(text) ?? false,
    { timeout, timeoutMsg: `${selector} never showed "${text}"` },
  );
}

/** The Vault's state in the status bar: `Vault locked` or `Vault unlocked`. */
export function vaultStatus(): Promise<string | null> {
  return textOf("footer .vault-status");
}

/**
 * Answers the open Vault password dialog and presses its primary button.
 * The dialog that creates the password has two fields (the password and
 * "Repeat it"): both get `password`.
 */
export async function answerVaultPassword(password: string): Promise<void> {
  const dialog = $('[role="dialog"]');
  await dialog.waitForDisplayed({ timeout: 15_000 });
  const fields = await dialog.$$('input[type="password"]');
  for (const field of fields) await field.setValue(password);
  await dialog
    .$(`button=${fields.length > 1 ? "Create password" : "Unlock"}`)
    .click();
}

/** Opens the command palette with its shortcut and waits for it. */
export async function openPalette(): Promise<void> {
  await browser.keys([MOD, "k"]);
  await $('[role="dialog"]').$('input[role="combobox"]').waitForDisplayed();
}

/** Types `text` into the open palette's search box. */
export async function paletteSearch(text: string): Promise<void> {
  await $('[role="dialog"]').$('input[role="combobox"]').setValue(text);
}

/** The label of the palette's active row, read in the page. */
export function paletteActive(): Promise<string | null> {
  return browser.execute(() => {
    const box = document.querySelector('[role="combobox"]');
    const id = box?.getAttribute("aria-activedescendant");
    return id
      ? (document.getElementById(id)?.querySelector(".label")?.textContent ??
          null)
      : null;
  });
}

/** Waits until no dialog is open. */
export async function waitForNoDialog(): Promise<void> {
  await browser.waitUntil(
    async () => !(await $('[role="dialog"]').isExisting()),
    { timeoutMsg: "a dialog stayed open" },
  );
}

type Row = WebdriverIO.Element | ChainablePromiseElement;

/** Where over a row a drag is let go: before it, in it (a folder), after it. */
export type DropPlace = "above" | "into" | "below";

// Rows are 28 px high; the drop rules in state/drag.svelte.ts read the top
// quarter of a folder or half of a request as "above", the middle half of a
// folder as "into", and the rest as "below".
const DROP_OFFSET: Record<DropPlace, number> = {
  above: 3,
  into: 14,
  below: 25,
};

const DRAG_STEPS = 8;

/**
 * Drags `source` over `target` with the pointer: press, move away past the
 * drag threshold, move to `place` of the target in steps, release. The tree
 * drags with pointer events (WebDriver cannot drive HTML drag and drop in
 * WebKitGTK or WebView2). The whole move stays under the 600 ms a closed
 * folder needs to open under the pointer.
 */
export async function dragTo(
  source: Row,
  target: Row,
  place: DropPlace,
): Promise<void> {
  const from = await (await source).getLocation();
  const fromSize = await (await source).getSize();
  const to = await (await target).getLocation();
  const toSize = await (await target).getSize();
  const start = {
    x: Math.round(from.x + fromSize.width / 2),
    y: Math.round(from.y + fromSize.height / 2),
  };
  const end = {
    x: Math.round(to.x + toSize.width / 2),
    y: Math.round(to.y + DROP_OFFSET[place]),
  };
  const action = browser
    .action("pointer", { parameters: { pointerType: "mouse" } })
    .move({ x: start.x, y: start.y })
    .down()
    // Past the 4 px threshold, so the press becomes a drag.
    .move({ x: start.x + 8, y: start.y, duration: 20 });
  for (let i = 1; i <= DRAG_STEPS; i++) {
    action.move({
      x: Math.round(start.x + 8 + ((end.x - start.x - 8) * i) / DRAG_STEPS),
      y: Math.round(start.y + ((end.y - start.y) * i) / DRAG_STEPS),
      duration: 20,
    });
  }
  await action.pause(50).up().perform();
  await browser.releaseActions();
}
