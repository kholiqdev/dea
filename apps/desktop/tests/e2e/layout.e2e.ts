// The layout of the Requests screen (F1-REQ-03, F1-FON-01): the request bar
// at a narrow window, the response header on one row, the divider between
// the request and the response, production on the bar and the tabs, the
// second send past a warning, and the open request marked in the tree. Each
// case opens its own small project, so none depends on another.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { Key } from "webdriverio";
import { MOD, textOf, typeKeys } from "./helpers.js";
import {
  activeTab,
  arrivals,
  chooseEnvironment,
  focusUrl,
  getRequest,
  openRequest,
  sendAndShow,
  sendFace,
  showRequest,
  tab,
  until,
  waitForSendToEnd,
  withWindowSize,
} from "./requests-support.js";
import {
  startTestServer,
  writeServerProject,
  type TestServer,
} from "./test-server.js";

const DIVIDER = ".split > .divider";

const work = mkdtempSync(join(tmpdir(), "dea-e2e-layout-"));
let server: TestServer;
let count = 0;

/** A project that sends to the test server, with more requests if given. */
function serverProject(requests: Record<string, string> = {}): string {
  return writeServerProject(`layout-${++count}`, work, server, requests);
}

/** The request's share of the split, as the divider reports it. */
async function splitShare(): Promise<number> {
  return Number(await $(DIVIDER).getAttribute("aria-valuenow"));
}

/** Gives the divider the keyboard focus, which a click would not on 1 px. */
async function focusDivider(): Promise<void> {
  await $(DIVIDER).waitForExist();
  await browser.execute((s) => {
    document.querySelector<HTMLElement>(s)?.focus();
  }, DIVIDER);
  await browser.waitUntil(
    () =>
      browser.execute(
        (s) => document.activeElement === document.querySelector(s),
        DIVIDER,
      ),
    { timeoutMsg: "the divider did not take focus" },
  );
}

describe("the Requests layout", () => {
  before(async () => {
    server = await startTestServer();
  });
  after(async () => {
    await server.stop();
    rmSync(work, { recursive: true, force: true });
  });

  it("a_very_long_url_keeps_send_in_view", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    await withWindowSize(1024, 640, async () => {
      await focusUrl();
      await typeKeys("/" + "segment-".repeat(50));
      // The final URL follows the field once the engine has looked at it.
      await browser.waitUntil(
        () =>
          browser.execute(
            () =>
              document
                .querySelector('button[aria-label="Copy the final URL"]')
                ?.getAttribute("title")
                ?.endsWith("segment-") ?? false,
          ),
        { timeoutMsg: "the final URL never showed the long path" },
      );
      const { send, bar, final, width } = await browser.execute(() => {
        const box = (s: string) =>
          document.querySelector(s)!.getBoundingClientRect();
        return {
          send: box("button[data-send]"),
          bar: box(".line"),
          final: box('button[aria-label="Copy the final URL"]'),
          width: window.innerWidth,
        };
      });
      expect(send.right).toBeLessThanOrEqual(width);
      expect(bar.height).toBe(56);
      expect(final.height).toBeLessThanOrEqual(20);
      expect(final.right).toBeLessThanOrEqual(width);
    });
  });

  it("the_response_header_stays_on_one_row", async () => {
    await openRequest(
      serverProject({
        "long.yaml": getRequest("Long reason", "/long-reason"),
      }),
      "staging",
      "Long reason",
    );
    await withWindowSize(1024, 640, async () => {
      await sendAndShow();
      const { toolbar, copy } = await browser.execute(() => {
        const bar = document.querySelector(
          '[role="toolbar"][aria-label="Response"]',
        )!;
        return {
          toolbar: bar.getBoundingClientRect(),
          copy: bar
            .querySelector('button[aria-label="Copy response"]')!
            .getBoundingClientRect(),
        };
      });
      expect(toolbar.height).toBe(40);
      expect(copy.right).toBeLessThanOrEqual(toolbar.right);
    });
  });

  it("the_url_field_has_room_at_1024", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    await withWindowSize(1024, 640, async () => {
      // The module sidebar steps aside, which gives the field its room.
      await $('aside[aria-label="Requests sidebar"]').waitForExist({
        reverse: true,
        timeoutMsg: "the sidebar stayed at 1024 px",
      });
      await browser.waitUntil(
        async () => (await $('input[aria-label="URL"]').getSize()).width >= 600,
        { timeoutMsg: "the URL field is narrower than 600 px" },
      );
    });
  });

  it("a_resized_split_is_remembered", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    try {
      await focusDivider();
      expect(await splitShare()).toBe(44);
      // The request keeps 360 px, so a 950 px area gives no room to move
      // left of 38%: the divider goes right, where 50% is within reach.
      expect(
        Number(await $(DIVIDER).getAttribute("aria-valuemax")),
      ).toBeGreaterThanOrEqual(50);
      await typeKeys(Key.ArrowRight.repeat(3));
      await browser.waitUntil(async () => (await splitShare()) === 50, {
        timeoutMsg: "three ArrowRight did not move the divider to 50",
      });
      const shares = () =>
        browser.execute(() => {
          const split = document.querySelector(".split")!;
          const editor = split.querySelector(":scope > .editor")!;
          return (
            (editor.getBoundingClientRect().width * 100) /
            split.getBoundingClientRect().width
          );
        });
      expect(Math.abs((await shares()) - 50)).toBeLessThanOrEqual(1);

      // The share is the app's, so a reopened tab keeps it.
      await $('button[aria-label="Close Shipment by ID"]').click();
      await tab("Shipment by ID").waitForExist({ reverse: true });
      await showRequest("Shipment by ID");
      await browser.waitUntil(async () => (await splitShare()) === 50, {
        timeoutMsg: "the reopened tab lost the share",
      });

      await focusDivider();
      await browser.keys(Key.Enter);
      await browser.waitUntil(async () => (await splitShare()) === 44, {
        timeoutMsg: "Enter did not reset the divider to 44",
      });
    } finally {
      // Saved in the settings: the cases after this one start from 44.
      if ((await $(DIVIDER).isExisting()) && (await splitShare()) !== 44) {
        await focusDivider();
        await browser.keys(Key.Enter);
      }
    }
  });

  it("production_says_so_on_the_send_button", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    await chooseEnvironment("production");
    await browser.waitUntil(
      async () => ((await sendFace()) ?? "").includes("Send to production"),
      { timeoutMsg: "the button does not say production" },
    );
    await $("[data-tab][data-production]").waitForExist({
      timeoutMsg: "no tab is marked as production",
    });

    await chooseEnvironment("staging");
    await browser.waitUntil(
      async () => /^Send(?! to production)/.test((await sendFace()) ?? ""),
      { timeoutMsg: "the button still says production" },
    );
    await $("[data-production]").waitForExist({
      reverse: true,
      timeoutMsg: "a tab is still marked as production",
    });
  });

  it("a_second_mod_enter_sends_past_the_warning", async () => {
    // The URL bar reads {{baseUrl}}/echo?carrier={{carrierCode}}.
    const root = serverProject({
      "track-shipment.yaml": getRequest("Track shipment", "/echo", {
        carrier: "{{carrierCode}}",
      }),
    });
    await openRequest(root, "staging", "Track shipment");
    const sent = arrivals(server, "/echo");

    await focusUrl();
    await browser.keys([MOD, Key.Enter]);
    const banner = $('//*[@role="status"][contains(., "is not defined")]');
    await banner.waitForDisplayed({ timeoutMsg: "no warning came" });
    expect(sent()).toBe(0);

    // A send within 400 ms of the warning is ignored: wait past that.
    await browser.pause(500);
    await focusUrl();
    await browser.keys([MOD, Key.Enter]);
    await until(() => sent() === 1, "the second Mod+Enter to send");
    await waitForSendToEnd();
  });

  it("the_sidebar_marks_the_open_request", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    await showRequest("Create shipment");
    const marked = '[role="treeitem"][aria-current="page"] span.name';
    await browser.waitUntil(
      async () => (await textOf(marked)) === "Create shipment",
      {
        timeoutMsg: "the tree does not mark the open request",
      },
    );

    await tab("Shipment by ID").click();
    await browser.waitUntil(
      async () => (await activeTab()) === "Shipment by ID",
    );
    await browser.waitUntil(
      async () => (await textOf(marked)) === "Shipment by ID",
      {
        timeoutMsg: "the mark did not follow the tab",
      },
    );
    await tab("Create shipment").click();
    await browser.waitUntil(
      async () => (await textOf(marked)) === "Create shipment",
      {
        timeoutMsg: "the mark did not follow the tab back",
      },
    );
  });
});
