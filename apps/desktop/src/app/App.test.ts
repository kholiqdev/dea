import { screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";
import { settings } from "$lib/settings/settings.svelte";
import { fakeIpc } from "./shell/test-support";
import { start } from "./start";

// The OS theme, which the test can switch. Each query is a real
// EventTarget, as a MediaQueryList is in a browser.
let osDark = false;
class FakeMediaQueryList extends EventTarget {
  constructor(readonly media: string) {
    super();
  }
  get matches(): boolean {
    return this.media === "(prefers-color-scheme: dark)" ? osDark : false;
  }
}
const queries: FakeMediaQueryList[] = [];
vi.stubGlobal("matchMedia", (query: string) => {
  const q = new FakeMediaQueryList(query);
  queries.push(q);
  return q;
});

function switchOs(dark: boolean): void {
  osDark = dark;
  for (const q of queries)
    if (q.media === "(prefers-color-scheme: dark)")
      q.dispatchEvent(new Event("change"));
  flushSync();
}

const root = document.documentElement;

test("starts on Welcome when the window has no project", async () => {
  const log = fakeIpc();
  const target = document.body.appendChild(document.createElement("div"));
  await start(target);

  expect(
    screen.queryByRole("navigation", { name: "Modules" }),
  ).not.toBeInTheDocument();
  expect(
    await screen.findByRole("heading", { name: "Start a project" }),
  ).toBeInTheDocument();
  expect(screen.getByRole("banner")).toHaveTextContent("No project open");
  expect(root.dataset.theme).toBe("light");
  expect(log.savedUiStates).toEqual([]);
});

test("appearance changes apply at once", async () => {
  await settings.update({ appearance: { theme: "dark", fontSize: 16 } });
  flushSync();
  expect(root.dataset.theme).toBe("dark");
  expect(root.style.getPropertyValue("--font-size")).toBe("16px");
  await settings.update({ appearance: { theme: "light", fontSize: 13 } });
  flushSync();
  expect(root.dataset.theme).toBe("light");
});

test("Match system follows a change of the OS theme", async () => {
  await settings.update({ appearance: { theme: "system" } });
  flushSync();
  expect(root.dataset.theme).toBe("light");
  switchOs(true);
  expect(root.dataset.theme).toBe("dark");
  switchOs(false);
  expect(root.dataset.theme).toBe("light");
  // An explicit choice ignores the OS.
  await settings.update({ appearance: { theme: "dark" } });
  flushSync();
  switchOs(false);
  expect(root.dataset.theme).toBe("dark");
});

test("Settings opens with Mod+, and Back returns to Welcome", async () => {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: ",", code: "Comma", ctrlKey: true }),
  );
  const nav = await screen.findByRole("navigation", {
    name: "Settings sections",
  });
  expect(nav).toHaveTextContent("Appearance");
  expect(
    await screen.findByRole("heading", { name: "Appearance", level: 1 }),
  ).toBeInTheDocument();
  screen.getByRole("button", { name: "Back" }).click();
  expect(
    await screen.findByRole("heading", { name: "Start a project" }),
  ).toBeInTheDocument();
});
