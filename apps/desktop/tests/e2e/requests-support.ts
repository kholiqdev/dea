// Driving a request tab end to end, for the specs that send to the test
// server: request files for its project, the environment picker, opening a
// request of Shipments, sending, and waiting for what the server received.

import { $, browser } from "@wdio/globals";
import {
  environmentName,
  openProjectAt,
  textOf,
  waitForText,
} from "./helpers.js";
import type { TestServer } from "./test-server.js";

export const RESPONSE = 'section[aria-label="Response"]';
export const TOOLBAR = '[role="toolbar"][aria-label="Response"]';

/** Quotes `text` for YAML, so `{{name}}` stays a string. */
function quoted(text: string): string {
  return `'${text.replaceAll("'", "''")}'`;
}

/**
 * A request file for Shipments (see writeServerProject): `GET
 * {{baseUrl}}path`, with `params` as its query.
 */
export function getRequest(
  name: string,
  path: string,
  params: Record<string, string> = {},
): string {
  const query = Object.entries(params)
    .map(([key, value]) => `  - name: ${key}\n    value: ${quoted(value)}\n`)
    .join("");
  return (
    `name: ${name}\nmethod: GET\nurl: ${quoted(`{{baseUrl}}${path}`)}\n` +
    (query ? `params:\n${query}` : "")
  );
}

/** Counts the requests to `path` that reach `server` from now on. */
export function arrivals(server: TestServer, path: string): () => number {
  const before = server.received(path).length;
  return () => server.received(path).length - before;
}

/** Waits for `condition`, checked here rather than in the page. */
export async function until(
  condition: () => boolean,
  what: string,
  timeout = 30_000,
): Promise<void> {
  await browser.waitUntil(condition, {
    timeout,
    interval: 50,
    timeoutMsg: `never happened: ${what}`,
  });
}

/** Chooses `name` in the title bar's environment picker. */
export async function chooseEnvironment(name: string): Promise<void> {
  await $('header button[title="Environment for requests"]').click();
  await $('[role="menu"]').$(`button*=${name}`).click();
  await browser.waitUntil(async () => (await environmentName()) === name);
}

/** The tree row named `name`. */
export function row(name: string) {
  return $(
    `//*[@role="treeitem"][.//span[contains(@class,"name") and normalize-space(.)="${name}"]]`,
  );
}

/** Opens a folder row that is closed. */
export async function expand(name: string): Promise<void> {
  const item = row(name);
  await item.waitForDisplayed();
  if ((await item.getAttribute("aria-expanded")) === "false")
    await item.click();
  await browser.waitUntil(
    async () => (await item.getAttribute("aria-expanded")) === "true",
  );
}

/** The title of the active tab. */
export function activeTab(): Promise<string | null> {
  return textOf(
    '[role="tablist"][aria-label="Open tabs"] [aria-selected="true"] .title',
  );
}

/** The tab titled `title` in the tab bar. */
export function tab(title: string) {
  return $(
    `//*[@role="tablist" and @aria-label="Open tabs"]//button[@role="tab"][.//span[normalize-space(.)="${title}"]]`,
  );
}

/**
 * Shows the request `name` of Shipments, in its tab or a new one. Another
 * tab's editor may still be there, so it waits for this tab first.
 */
export async function showRequest(name: string): Promise<void> {
  await row(name).click();
  await browser.waitUntil(async () => (await activeTab()) === name, {
    timeoutMsg: `${name} did not open`,
  });
  await $('[role="tablist"][aria-label="Request sections"]').waitForDisplayed();
}

/** Opens `root` in `environment`, and `request` of Shipments in a tab. */
export async function openRequest(
  root: string,
  environment: string,
  request: string,
): Promise<void> {
  await openProjectAt(root);
  await chooseEnvironment(environment);
  await $('button[data-module="requests"]').click();
  await expand("Shipments");
  await showRequest(request);
}

/**
 * Puts the text cursor at the end of the URL field. Its variable chips sit
 * in a layer over the input and take a pointer click themselves (they focus
 * the input too), so WebDriver would call a click on the input intercepted.
 */
export async function focusUrl(): Promise<void> {
  const selector = 'input[aria-label="URL"]';
  await $(selector).waitForDisplayed();
  await browser.execute((s) => {
    const input = document.querySelector<HTMLInputElement>(s);
    if (!input) throw new Error(`no ${s}`);
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  }, selector);
  await browser.waitUntil(
    () =>
      browser.execute(
        (s) => document.activeElement === document.querySelector(s),
        selector,
      ),
    { timeoutMsg: "the URL field did not take focus" },
  );
}

/** Send, which reads Cancel while the request is on its way. */
export function sendButton() {
  return $("button[data-send]");
}

/**
 * What the send button reads now: `Send`, `Send to production` or
 * `Cancel`. Both faces stay in the button, the one not in use hidden, so
 * this reads the one in use.
 */
export function sendFace(): Promise<string | null> {
  return textOf("button[data-send] .face:not(.off)");
}

/** The timing bar of the response header, which opens the timing legend. */
export function timingBar() {
  return $(RESPONSE).$('button[aria-label^="Timing,"]');
}

/**
 * The JSONPath filter field, once it shows. It is in the second row of a
 * JSON response at every width of the pane.
 */
export async function openFilter() {
  const field = $('input[aria-label="Filter with JSONPath"]');
  await field.waitForDisplayed();
  return field;
}

/**
 * Runs `run` with the window at `width` × `height`, and puts it back to
 * 1280 × 800 afterwards, also when `run` fails: the size belongs to the
 * session, not to the case.
 */
export async function withWindowSize(
  width: number,
  height: number,
  run: () => Promise<void>,
): Promise<void> {
  const inner = () => browser.execute(() => window.innerWidth);
  try {
    await browser.setWindowSize(width, height);
    await browser.waitUntil(async () => (await inner()) <= width, {
      timeoutMsg: `the window did not shrink to ${width}`,
    });
    await run();
  } finally {
    await browser.setWindowSize(1280, 800);
    // Wider than the 1024 px under which the shell hides its sidebar.
    await browser.waitUntil(async () => (await inner()) > 1024, {
      timeoutMsg: "the window did not return to 1280 × 800",
    });
  }
}

export function productionDialog() {
  return $(
    '//*[@role="dialog"][.//h2[normalize-space(.)="Send this request to production?"]]',
  );
}

/**
 * Whether the tab is sending. A confirmation to answer is not sending yet:
 * the pane reads `Waiting for confirmation` and is not busy.
 */
export function sending(): Promise<boolean> {
  return browser.execute(
    (s) => document.querySelector(s)?.getAttribute("aria-busy") === "true",
    RESPONSE,
  );
}

/** Waits until the send under way, if any, ended. */
export async function waitForSendToEnd(): Promise<void> {
  await browser.waitUntil(async () => !(await sending()), {
    timeout: 30_000,
    timeoutMsg: "the send did not end",
  });
}

/**
 * Sends the request of the tab shown, which asks `server` for `path`, and
 * waits until its response is in. In production it confirms first.
 */
export async function sendTo(
  server: TestServer,
  path: string,
  production = false,
): Promise<void> {
  const sent = arrivals(server, path);
  await sendButton().click();
  if (production) {
    const dialog = productionDialog();
    await dialog.waitForDisplayed({ timeout: 30_000 });
    await dialog.$("button*=Send to production").click();
  }
  await until(() => sent() === 1, `the request to ${path} to arrive`);
  await waitForSendToEnd();
}

/** Sends a request whose answer is quick, and waits for its toolbar. */
export async function sendAndShow(): Promise<void> {
  await sendButton().click();
  await waitForText(TOOLBAR, "200 OK", 60_000);
}
