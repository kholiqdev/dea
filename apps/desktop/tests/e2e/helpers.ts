import { $, browser } from "@wdio/globals";
import { Key } from "webdriverio";

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
