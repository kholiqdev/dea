// Sending requests (F1-REQ-03 #5, F1-REQ-05 #3, F1-REQ-06 #1, F1-REQ-08,
// F1-REQ-12, and the Requests part of F1-FON-10): the response, Mod+Enter,
// Cancel, undefined variables, production from every path, and a large
// upload. A local server sees what arrives, so a case can tell that
// nothing was sent. Each case opens its own small project, so none depends
// on another.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { Key } from "webdriverio";
import {
  MOD,
  environmentName,
  longestFrameDuring,
  openPalette,
  openProjectAt,
  paletteActive,
  paletteSearch,
  textOf,
  waitForText,
} from "./helpers.js";
import {
  startTestServer,
  writeServerProject,
  type TestServer,
} from "./test-server.js";

const RESPONSE = 'section[aria-label="Response"]';
const TOOLBAR = '[role="toolbar"][aria-label="Response"]';
const FIFTY_MIB = 50 * 1024 * 1024;

const work = mkdtempSync(join(tmpdir(), "dea-e2e-send-"));
let server: TestServer;
let count = 0;

/** A project that sends to the test server, with more requests if given. */
function serverProject(requests: Record<string, string> = {}): string {
  return writeServerProject(`send-${++count}`, work, server, requests);
}

/** Counts the requests to `path` that reach the server from now on. */
function arrivals(path: string): () => number {
  const before = server.received(path).length;
  return () => server.received(path).length - before;
}

async function until(
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
async function chooseEnvironment(name: string): Promise<void> {
  await $('header button[title="Environment for requests"]').click();
  await $('[role="menu"]').$(`button*=${name}`).click();
  await browser.waitUntil(async () => (await environmentName()) === name);
}

/** The tree row named `name`. */
function row(name: string) {
  return $(
    `//*[@role="treeitem"][.//span[contains(@class,"name") and normalize-space(.)="${name}"]]`,
  );
}

/** Opens a folder row that is closed. */
async function expand(name: string): Promise<void> {
  const item = row(name);
  await item.waitForDisplayed();
  if ((await item.getAttribute("aria-expanded")) === "false")
    await item.click();
  await browser.waitUntil(
    async () => (await item.getAttribute("aria-expanded")) === "true",
  );
}

/** Opens `root` in `environment`, and `request` of Shipments in a tab. */
async function openRequest(
  root: string,
  environment: string,
  request: string,
): Promise<void> {
  await openProjectAt(root);
  await chooseEnvironment(environment);
  await $('button[data-module="requests"]').click();
  await expand("Shipments");
  await row(request).click();
  await $('[role="tablist"][aria-label="Request sections"]').waitForDisplayed();
}

/** Send, which reads Cancel while the request is on its way. */
function sendButton() {
  return $('//button[@aria-label="Save request"]/preceding-sibling::button[1]');
}

function productionDialog() {
  return $(
    '//*[@role="dialog"][.//h2[normalize-space(.)="Send this request to production?"]]',
  );
}

/** Whether the tab is sending, a confirmation still to answer included. */
function sending(): Promise<boolean> {
  return browser.execute(
    (s) => document.querySelector(s)?.getAttribute("aria-busy") === "true",
    RESPONSE,
  );
}

/** Waits until the send under way, if any, ended. */
async function waitForSendToEnd(): Promise<void> {
  await browser.waitUntil(async () => !(await sending()), {
    timeout: 30_000,
    timeoutMsg: "the send did not end",
  });
}

describe("sending requests", () => {
  before(async () => {
    server = await startTestServer();
  });
  after(async () => {
    await server.stop();
    rmSync(work, { recursive: true, force: true });
  });

  it("sends_with_the_button_and_shows_the_response", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    const sent = arrivals("/json");

    await sendButton().click();
    await waitForText(TOOLBAR, "200 OK", 30_000);
    expect(sent()).toBe(1);
    const toolbar = await textOf(TOOLBAR);
    expect(toolbar).toContain("ms");
    expect(toolbar).toContain("HTTP/1.1");
    expect(
      await $(RESPONSE).$('[role="img"]').getAttribute("aria-label"),
    ).toMatch(/^Timing,/);
    // The body opens in the editor, which loads on first use.
    await waitForText(
      '.cm-content[aria-label="Response body"]',
      "shp_8KQ2M4",
      30_000,
    );
  });

  it("mod_enter_sends_from_the_url_field_and_the_body_editor", async () => {
    await openRequest(serverProject(), "staging", "Create shipment");
    const sent = arrivals("/echo");

    await $('input[aria-label="URL"]').click();
    await browser.keys([MOD, Key.Enter]);
    await until(() => sent() === 1, "Mod+Enter in the URL to send");
    await waitForSendToEnd();

    await $('[role="tablist"][aria-label="Request sections"]')
      .$("button=Body")
      .click();
    const body = $('.cm-content[aria-label="Body"]');
    await body.waitForDisplayed({ timeout: 30_000 });
    await body.click();
    await browser.keys([MOD, Key.Enter]);
    await until(() => sent() === 2, "Mod+Enter in the body to send");
    await waitForSendToEnd();
    expect(server.received("/echo").at(-1)?.method).toBe("POST");
  });

  it("cancel_closes_the_connection", async () => {
    const path = "/delay/10000";
    const root = serverProject({
      "slow-shipment.yaml": `name: Slow shipment\nmethod: GET\nurl: '{{baseUrl}}${path}'\n`,
    });
    await openRequest(root, "staging", "Slow shipment");
    const sent = arrivals(path);
    const abortedBefore = server.aborted(path);

    await sendButton().click();
    await until(() => sent() === 1, "the slow request to arrive");
    await browser.waitUntil(
      async () => (await sendButton().getText()).includes("Cancel"),
      { timeoutMsg: "Send did not turn into Cancel" },
    );
    // A second click within the double-click time would be ignored as the
    // end of a double click on Send.
    await browser.pause(1_000);
    await sendButton().click();
    // Within a second here; the Rust tests hold it to 200 ms.
    await until(
      () => server.aborted(path) === abortedBefore + 1,
      "the connection to close",
      1_000,
    );
    await waitForText(RESPONSE, "Cancelled after");
  });

  it("undefined_variable_warns_before_sending", async () => {
    // The URL bar reads {{baseUrl}}/echo?carrier={{carrierCode}}.
    const root = serverProject({
      "track-shipment.yaml":
        "name: Track shipment\nmethod: GET\nurl: '{{baseUrl}}/echo'\n" +
        "params:\n  - name: carrier\n    value: '{{carrierCode}}'\n",
    });
    await openRequest(root, "staging", "Track shipment");
    const sent = arrivals("/echo");

    await sendButton().click();
    const banner = $('//*[@role="status"][contains(., "is not defined")]');
    await banner.waitForDisplayed({ timeout: 30_000 });
    expect(await banner.getText()).toContain("carrierCode");
    expect(sent()).toBe(0);

    await banner.$("button*=Send anyway").click();
    await until(() => sent() === 1, "Send anyway to send");
  });

  it("production_asks_before_sending_from_every_path", async () => {
    await openRequest(serverProject(), "production", "Shipment by ID");
    const paths: [string, () => Promise<void>][] = [
      ["the Send button", () => sendButton().click()],
      [
        "Mod+Enter",
        async () => {
          await $('input[aria-label="URL"]').click();
          await browser.keys([MOD, Key.Enter]);
        },
      ],
      [
        "the palette",
        async () => {
          await openPalette();
          await paletteSearch("Send request");
          await browser.waitUntil(
            async () => (await paletteActive()) === "Send request",
          );
          await browser.keys(Key.Enter);
        },
      ],
    ];

    for (const [from, send] of paths) {
      const sent = arrivals("/json");
      await send();
      const dialog = productionDialog();
      await dialog.waitForDisplayed({
        timeout: 30_000,
        timeoutMsg: `${from} sent without asking`,
      });
      // The engine waits for the answer: nothing has left yet.
      expect(sent()).toBe(0);
      await dialog.$("button*=Send to production").click();
      await until(() => sent() === 1, `the confirmed send from ${from}`);
      await waitForSendToEnd();
    }
  });

  it("enter_in_the_production_dialog_does_not_send", async () => {
    await openRequest(serverProject(), "production", "Shipment by ID");
    const sent = arrivals("/json");

    await sendButton().click();
    const dialog = productionDialog();
    await dialog.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(
      () =>
        browser.execute(() => {
          const focused = document.activeElement;
          return (
            !!focused?.closest('[role="dialog"]') &&
            (focused.textContent ?? "").includes("Cancel")
          );
        }),
      { timeoutMsg: "Cancel did not get the focus" },
    );
    await browser.keys(Key.Enter);
    await dialog.waitForExist({
      reverse: true,
      timeoutMsg: "Enter left the dialog open",
    });
    await waitForSendToEnd();
    expect(sent()).toBe(0);
  });

  // Windows asks through a native dialog, which WebDriver cannot answer, so
  // this case runs on Linux, where Dea asks for its own Vault password. The
  // secret is only declared: the send stops before any value is read, and
  // no Vault password is made that vault.e2e.ts expects to make first.
  (process.platform === "win32" ? it.skip : it)(
    "cancelling_vault_authentication_sends_nothing",
    async () => {
      const root = serverProject({
        "shipment-with-token.yaml":
          "name: Shipment with token\nmethod: GET\nurl: '{{baseUrl}}/echo'\n" +
          "headers:\n  - name: Authorization\n    value: 'Bearer {{vault:PROD_TOKEN}}'\n",
      });
      writeFileSync(
        join(root, "vault.yaml"),
        "secrets:\n  - name: PROD_TOKEN\n    type: token\n",
      );
      await openRequest(root, "production", "Shipment with token");
      const sent = arrivals("/echo");

      await sendButton().click();
      const confirm = productionDialog();
      await confirm.waitForDisplayed({ timeout: 30_000 });
      expect(await textOf('[role="dialog"]')).toContain("production secrets");
      await confirm.$("button*=Send to production").click();

      // The Vault password, or the one to create on a first run.
      const vault = $(
        '//*[@role="dialog"][.//h2[contains(., "Vault password")]]',
      );
      await vault.waitForDisplayed({ timeout: 30_000 });
      await vault.$("button*=Cancel").click();
      await vault.waitForExist({ reverse: true });
      await waitForSendToEnd();
      expect(sent()).toBe(0);
    },
  );

  it("a_50_mb_upload_keeps_the_ui_responsive", async () => {
    const root = serverProject({
      "upload-label.yaml":
        "name: Upload label\nmethod: POST\nurl: '{{baseUrl}}/upload'\n" +
        "body:\n  mode: binary\n  file: data/big.bin\n",
    });
    mkdirSync(join(root, "data"));
    writeFileSync(join(root, "data", "big.bin"), Buffer.alloc(FIFTY_MIB));
    await openRequest(root, "staging", "Upload label");

    // The server reads about 20 MiB/s, so the upload takes 2.5 s.
    const longest = await longestFrameDuring(async () => {
      await sendButton().click();
      await waitForText(TOOLBAR, "200 OK", 60_000);
    });
    expect(longest).toBeLessThanOrEqual(100); // F1-REQ-03 #5
    expect(server.received("/upload")[0]?.bodyBytes).toBe(FIFTY_MIB);
  });
});
