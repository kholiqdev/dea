// Reading responses (F1-REQ-07, F1-REQ-09, F1-REQ-16): Pretty, Raw and
// Preview for every kind the test server sends, the JSONPath filter, frames
// while scrolling a 10 MB JSON and a 12 MiB body in the virtual viewer,
// server-sent events, the cookie jar per environment, and where the
// response sits. Each case opens its own small project, so none depends on
// another.

import { existsSync, mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  MOD,
  logFrame,
  longestFrameDuring,
  saveNextTo,
  textOf,
  typeKeys,
  waitForText,
} from "./helpers.js";
import {
  RESPONSE,
  activeTab,
  arrivals,
  chooseEnvironment,
  getRequest,
  openFilter,
  openRequest,
  sendAndShow,
  sendTo,
  showRequest,
  tab,
  until,
} from "./requests-support.js";
import {
  SHIPMENT_JSON,
  SHIPMENT_XML,
  hitHtml,
  startTestServer,
  writeServerProject,
  type TestServer,
} from "./test-server.js";

/** The body in the editor (Pretty, Raw) and in the virtual viewer. */
const BODY_EDITOR = '.cm-content[aria-label="Response body"]';
const BODY_VIEWER = '[role="document"][aria-label="Response body"]';

/** The longest frame allowed while scrolling (F1-REQ-16 #3). */
const FRAME_BUDGET_MS = 50;
/**
 * The longest frame allowed for a jump to the middle of 10 MB of JSON. The
 * Linux runner (WebKitGTK under Xvfb) takes 43 to 72 ms for it on `main` and
 * on this branch alike, so it is held to 75 ms there to still catch a real
 * regression; everywhere else it keeps the scrolling budget.
 */
const JUMP_BUDGET_MS = process.platform === "linux" ? 75 : FRAME_BUDGET_MS;
const SCROLL_STEPS = 20;
/** Between two scroll steps, so each one is drawn before the next. */
const SCROLL_PAUSE_MS = 50;
const TEN_MB = 10_000_000;
const TWELVE_MIB = 12 * 1024 * 1024;

const work = mkdtempSync(join(tmpdir(), "dea-e2e-response-"));
let server: TestServer;
let count = 0;

/** A project that sends to the test server, with more requests if given. */
function serverProject(requests: Record<string, string> = {}): string {
  return writeServerProject(`response-${++count}`, work, server, requests);
}

/** The body view chosen, such as `Pretty`. */
function bodyView(): Promise<string | null> {
  return textOf(
    '[role="radiogroup"][aria-label="Body view"] [aria-checked="true"]',
  );
}

async function waitForView(label: string): Promise<void> {
  await browser.waitUntil(async () => (await bodyView()) === label, {
    timeout: 30_000,
    timeoutMsg: `the body is not in ${label}`,
  });
}

async function chooseView(label: string): Promise<void> {
  await $('[role="radiogroup"][aria-label="Body view"]')
    .$(`button=${label}`)
    .click();
  await waitForView(label);
}

/** The lines the body editor draws, which is all of a short body. */
function bodyLines(): Promise<string[]> {
  return browser.execute(
    (s) =>
      [...document.querySelectorAll(`${s} .cm-line`)].map(
        (line) => line.textContent ?? "",
      ),
    BODY_EDITOR,
  );
}

/** Waits until the body editor's lines pass `check`, and gives them. */
async function waitForBody(
  what: string,
  check: (lines: string[]) => boolean,
  timeout = 30_000,
): Promise<string[]> {
  let lines: string[] = [];
  try {
    await browser.waitUntil(async () => check((lines = await bodyLines())), {
      timeout,
      interval: 100,
    });
  } catch (cause) {
    throw new Error(
      `the body never ${what}. It shows:\n${lines.slice(0, 30).join("\n")}`,
      { cause },
    );
  }
  return lines;
}

/** The fold markers in the gutter of the body editor. */
function foldMarkers(): Promise<number> {
  return browser.execute(
    (s) =>
      document
        .querySelector(s)
        ?.closest(".cm-editor")
        ?.querySelectorAll('.cm-foldGutter span[title="Fold line"]').length ??
      0,
    BODY_EDITOR,
  );
}

type Scroll = { top: number; height: number; client: number };

/**
 * Scrolls what shows `selector` (the editor's scroller, or the viewer
 * itself, or else the first box around it that scrolls) to `top`, or only
 * reads it when `top` is null.
 */
function scrollBody(selector: string, top: number | null): Promise<Scroll> {
  return browser.execute(
    (s, to) => {
      const found = document.querySelector(s);
      if (!found) throw new Error(`nothing shows ${s}`);
      let scroller = found.closest(".cm-scroller") ?? found;
      for (let el: Element | null = scroller; el; el = el.parentElement) {
        const overflow = getComputedStyle(el).overflowY;
        if (
          (overflow === "auto" || overflow === "scroll") &&
          el.scrollHeight > el.clientHeight + 1
        ) {
          scroller = el;
          break;
        }
      }
      if (to !== null) scroller.scrollTop = to;
      return {
        top: scroller.scrollTop,
        height: scroller.scrollHeight,
        client: scroller.clientHeight,
      };
    },
    selector,
    top,
  );
}

/**
 * Waits until the page draws for half a second with no frame over the
 * budget. Opening a body does work once (reading and checking it, the first
 * background parse), which is not what scrolling is timed for.
 */
async function waitForQuiet(): Promise<void> {
  await browser.waitUntil(
    async () =>
      (await longestFrameDuring(() => browser.pause(500))) <= FRAME_BUDGET_MS,
    { timeout: 30_000, interval: 100, timeoutMsg: "the page never settled" },
  );
}

/** How many fresh bodies a timed 10 MB case opens; their median is checked. */
const TIMED_TRIES = 3;

/**
 * Opens a fresh response of `request` `TIMED_TRIES` times (a new tab each
 * time, so nothing is parsed yet), times `measure` on each and returns the
 * median, logging every try. One frame stalled by a busy CI machine does not
 * decide the case; a slower app moves the median.
 */
async function medianOverFreshBodies(
  what: string,
  request: string,
  measure: () => Promise<number>,
): Promise<number> {
  const times: number[] = [];
  for (let i = 0; i < TIMED_TRIES; i++) {
    if (i > 0) {
      await $(`button[aria-label="Close ${request}"]`).click();
      await tab(request).waitForExist({ reverse: true });
      await showRequest(request);
    }
    await sendAndShow();
    // 10 MB is under 10 MiB, so the body stays in memory and opens Pretty.
    await waitForBody("showed the JSON", (l) => l.length > 1, 60_000);
    await waitForView("Pretty");
    await waitForQuiet();
    const longest = await measure();
    logFrame(`${what}, try ${i + 1}`, longest);
    times.push(longest);
  }
  times.sort((a, b) => a - b);
  return times[Math.floor(TIMED_TRIES / 2)]!;
}

type ViewerRow = { n: number; loaded: boolean };

/** The rows of the virtual viewer in view, by the number in their gutter. */
function rowsInView(): Promise<ViewerRow[]> {
  return browser.execute((s) => {
    const viewer = document.querySelector(s);
    if (!viewer) return [];
    const box = viewer.getBoundingClientRect();
    return [...viewer.querySelectorAll(".row")]
      .filter((row) => {
        const r = row.getBoundingClientRect();
        return r.bottom > box.top && r.top < box.bottom;
      })
      .map((row) => ({
        n: Number(row.querySelector(".gutter")?.textContent),
        loaded: (row.querySelector(".text")?.textContent ?? "") !== "",
      }));
  }, BODY_VIEWER);
}

type Box = { left: number; top: number; right: number; bottom: number };
type Split = { layout: string | null; editor: Box; response: Box };

/** Where the request editor and the response sit in the tab. */
function split(): Promise<Split | null> {
  return browser.execute(() => {
    const tab = document.querySelector(".split");
    const editor = tab?.querySelector(":scope > .editor");
    const response = tab?.querySelector(":scope > .response");
    if (!tab || !editor || !response) return null;
    const box = (el: Element) => {
      const r = el.getBoundingClientRect();
      return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    };
    return {
      layout: tab.getAttribute("data-layout"),
      editor: box(editor),
      response: box(response),
    };
  });
}

/** Waits until the response sits `layout` of the editor; gives both boxes. */
async function waitForLayout(layout: "right" | "below"): Promise<Split> {
  let shown: Split | null = null;
  await browser.waitUntil(
    async () => (shown = await split())?.layout === layout,
    { timeout: 30_000, timeoutMsg: `the response is not ${layout}` },
  );
  if (!shown) throw new Error("no request tab is shown");
  return shown;
}

/** The rows of the cookie manager: name, value, expires. */
function cookieRows(): Promise<string[][]> {
  return browser.execute(() =>
    [...document.querySelectorAll('table[aria-label="Cookies"] tbody tr')].map(
      (tr) =>
        [...tr.querySelectorAll("td")].map(
          (td) => td.textContent?.trim() ?? "",
        ),
    ),
  );
}

async function waitForCookie(name: string, value: string): Promise<void> {
  await browser.waitUntil(
    async () =>
      (await cookieRows()).some(([n, v]) => n === name && v === value),
    { timeout: 30_000, timeoutMsg: `the jar never showed ${name}=${value}` },
  );
}

/** The `Cookie` header of the last request to `/echo`, if it had one. */
function lastCookieHeader(): string | undefined {
  return server.received("/echo").at(-1)?.headers.cookie;
}

/** `HH:mm:ss.SSS` as milliseconds into the day. */
function msOfDay(time: string): number {
  const [h = 0, m = 0, s = 0] = time.split(":").map(Number);
  return ((h * 60 + m) * 60 + s) * 1000;
}

describe("reading responses", () => {
  before(async () => {
    server = await startTestServer();
  });
  after(async () => {
    await server.stop();
    rmSync(work, { recursive: true, force: true });
  });

  it("json_xml_and_html_open_pretty", async () => {
    const root = serverProject({
      "shipment-xml.yaml": getRequest("Shipment XML", "/xml"),
      "shipment-page.yaml": getRequest("Shipment page", "/html"),
    });

    // JSON: indented, with folding.
    await openRequest(root, "staging", "Shipment by ID");
    await sendAndShow();
    await waitForBody("showed the JSON", (l) =>
      l.join("").includes("shp_8KQ2M4"),
    );
    await waitForView("Pretty");
    await browser.waitUntil(async () => (await foldMarkers()) > 0, {
      timeoutMsg: "the JSON has no fold markers",
    });
    expect((await bodyLines()).length).toBeGreaterThan(1);
    await chooseView("Raw");
    await waitForBody(
      "showed the JSON as received",
      (l) => l.join("\n") === SHIPMENT_JSON,
    );

    // XML: one element per line, indented by depth.
    await showRequest("Shipment XML");
    await sendAndShow();
    const xml = await waitForBody("showed the XML indented", (l) =>
      l.includes("    <city>Jakarta</city>"),
    );
    await waitForView("Pretty");
    expect(xml).toContain("  <status>in_transit</status>");
    expect(xml).toContain("      <at>2026-11-03T02:14:00Z</at>");
    expect(xml.at(-1)).toBe("</shipment>");
    await chooseView("Raw");
    await waitForBody(
      "showed the XML as received",
      (l) => l.join("\n") === SHIPMENT_XML,
    );

    // HTML: one tag per line, the script copied as it is.
    await showRequest("Shipment page");
    await sendAndShow();
    const origin = server.url("");
    await waitForBody(
      "showed the HTML one tag per line",
      (l) =>
        l.join("\n") ===
        [
          "<p>hi</p>",
          `<script>fetch('${origin}/hit')</script>`,
          `<img src="${origin}/hit-img">`,
        ].join("\n"),
    );
    await waitForView("Pretty");
    await chooseView("Raw");
    await waitForBody(
      "showed the HTML as received",
      (l) => l.join("\n") === hitHtml(origin),
    );
  });

  it("html_preview_runs_no_script", async () => {
    const root = serverProject({
      "shipment-page.yaml": getRequest("Shipment page", "/html"),
    });
    await openRequest(root, "staging", "Shipment page");
    const hits = arrivals(server, "/hit");
    const imageHits = arrivals(server, "/hit-img");

    await sendAndShow();
    await waitForBody("showed the HTML", (l) => l.includes("<p>hi</p>"));
    await chooseView("Preview");
    const frame = 'iframe[title="Preview of the response"]';
    await $(frame).waitForExist({ timeout: 30_000 });
    const attributes = await browser.execute((s) => {
      const el = document.querySelector(s);
      return {
        sandbox: el?.getAttribute("sandbox") ?? null,
        srcdoc: el?.getAttribute("srcdoc") ?? null,
      };
    }, frame);
    expect(attributes.sandbox).toBe("");
    // The page points back at the server, so a script or image would reach it.
    expect(attributes.srcdoc).toBe(hitHtml(server.url("")));

    // Nothing may arrive: the sandbox stops the script, and the CSP the
    // frame inherits stops the image. A second is what either would take.
    await browser.pause(1_000);
    expect(hits()).toBe(0);
    expect(imageHits()).toBe(0);
  });

  it("image_and_pdf_preview", async () => {
    const root = serverProject({
      "label-image.yaml": getRequest("Label image", "/image.png"),
      "label-pdf.yaml": getRequest("Label PDF", "/doc.pdf"),
    });

    await openRequest(root, "staging", "Label image");
    await sendAndShow();
    // An image or a PDF has one view, Preview, so there is no view choice.
    const image = () =>
      browser.execute(() => {
        const img = document.querySelector<HTMLImageElement>(
          'img[alt="Response image"]',
        );
        return img?.complete ? img.naturalWidth : 0;
      });
    await browser.waitUntil(async () => (await image()) > 0, {
      timeout: 30_000,
      timeoutMsg: "the image never loaded",
    });
    expect(await image()).toBe(1);

    await showRequest("Label PDF");
    await sendAndShow();
    const pdf = () =>
      browser.execute((s) => {
        const pane = document.querySelector(s);
        return {
          canvases: pane?.querySelectorAll("canvas").length ?? 0,
          caption: [...(pane?.querySelectorAll("p") ?? [])].some(
            (p) => p.textContent?.trim() === "1 page",
          ),
          text: pane?.textContent ?? "",
        };
      }, RESPONSE);
    await browser.waitUntil(async () => (await pdf()).caption, {
      timeout: 30_000,
      timeoutMsg: 'the PDF never showed "1 page"',
    });
    const shown = await pdf();
    expect(shown.canvases).toBe(1);
    expect(shown.text).not.toContain("could not be shown");
  });

  it("filter_with_jsonpath", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    await sendAndShow();
    await waitForBody("showed the JSON", (l) =>
      l.join("").includes("shp_8KQ2M4"),
    );

    await (await openFilter()).click();
    await typeKeys("$.events[*].code");
    await browser.waitUntil(
      async () =>
        (await browser.execute(
          () =>
            document
              .querySelector('input[aria-label="Filter with JSONPath"]')
              ?.parentElement?.querySelector(".count")
              ?.textContent?.trim() ?? null,
        )) === "2 matches",
      { timeout: 30_000, timeoutMsg: 'the filter never showed "2 matches"' },
    );
    const shown = await waitForBody("showed what the filter selects", (l) => {
      const text = l.join("\n");
      return text.includes("picked_up") && !text.includes("shp_8KQ2M4");
    });
    expect(shown.join("\n")).toContain("departed_hub");
  });

  it("scrolling_10_mb_json_keeps_frames_under_50_ms", async () => {
    const path = `/json-size/${TEN_MB}`;
    const root = serverProject({
      "ten-megabytes.yaml": getRequest("Ten megabytes", path),
    });
    await openRequest(root, "staging", "Ten megabytes");

    const longest = await medianOverFreshBodies(
      "scrolling 10 MB JSON",
      "Ten megabytes",
      async () => {
        expect(await textOf(RESPONSE)).not.toContain("The response is");
        // A page at a time, like Page Down.
        const start = await scrollBody(BODY_EDITOR, null);
        const longest = await longestFrameDuring(async () => {
          for (let i = 1; i <= SCROLL_STEPS; i++) {
            await scrollBody(BODY_EDITOR, start.top + i * start.client);
            await browser.pause(SCROLL_PAUSE_MS);
          }
        });
        const end = await scrollBody(BODY_EDITOR, null);
        expect(end.top).toBeGreaterThan(start.top + 10 * start.client);
        return longest;
      },
    );
    expect(longest).toBeLessThanOrEqual(FRAME_BUDGET_MS); // F1-REQ-16 #3
  });

  it("jumping_to_the_middle_of_10_mb_json_keeps_frames_within_budget", async () => {
    const path = `/json-size/${TEN_MB}`;
    const root = serverProject({
      "ten-megabytes.yaml": getRequest("Ten megabytes", path),
    });
    await openRequest(root, "staging", "Ten megabytes");

    const longest = await medianOverFreshBodies(
      "jumping in 10 MB JSON",
      "Ten megabytes",
      async () => {
        // Like dragging the scrollbar: far past what was parsed, then on a
        // few pages from there.
        let middle = 0;
        const longest = await longestFrameDuring(async () => {
          const start = await scrollBody(BODY_EDITOR, null);
          middle = Math.round(start.height / 2);
          await scrollBody(BODY_EDITOR, middle);
          await browser.pause(SCROLL_PAUSE_MS);
          for (let i = 1; i <= 5; i++) {
            await scrollBody(BODY_EDITOR, middle + i * start.client);
            await browser.pause(SCROLL_PAUSE_MS);
          }
        });
        const end = await scrollBody(BODY_EDITOR, null);
        expect(end.top).toBeGreaterThan(middle / 2);
        return longest;
      },
    );
    expect(longest).toBeLessThanOrEqual(JUMP_BUDGET_MS);
  });

  it("a_12_mb_response_uses_the_virtual_viewer_and_saves_to_file", async () => {
    const path = `/json-line-size/${TWELVE_MIB}`;
    const root = serverProject({
      "twelve-megabytes.yaml": getRequest("Twelve megabytes", path),
    });
    await openRequest(root, "staging", "Twelve megabytes");

    await sendAndShow();
    await waitForText(`${RESPONSE} .card`, "The response is 12.0 MB", 60_000);
    await $(BODY_VIEWER).waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(
      async () => (await rowsInView()).some((r) => r.n === 1 && r.loaded),
      { timeout: 30_000, timeoutMsg: "the first row never loaded" },
    );
    await waitForQuiet();

    // To the middle in steps, until the rows there are read.
    const start = await scrollBody(BODY_VIEWER, null);
    const middle = (start.height - start.client) / 2;
    let shown: ViewerRow[] = [];
    const longest = await longestFrameDuring(async () => {
      for (let i = 1; i <= SCROLL_STEPS; i++) {
        await scrollBody(BODY_VIEWER, (middle * i) / SCROLL_STEPS);
        await browser.pause(SCROLL_PAUSE_MS);
      }
      await browser.waitUntil(
        async () => {
          shown = await rowsInView();
          return shown.length > 0 && shown.every((r) => r.loaded);
        },
        { timeout: 30_000, timeoutMsg: "the rows in the middle never loaded" },
      );
    });
    expect(Math.min(...shown.map((r) => r.n))).toBeGreaterThan(1_000);
    logFrame("scrolling the 12 MiB viewer", longest);
    expect(longest).toBeLessThanOrEqual(FRAME_BUDGET_MS); // F1-REQ-16 #3

    const saved = join(work, "twelve-megabytes.json");
    saveNextTo(saved);
    await $('//button[normalize-space(.)="Save to file"]').click();
    await until(
      () => existsSync(saved) && statSync(saved).size === TWELVE_MIB,
      "the saved file to hold the whole body",
      60_000,
    );
  });

  it("server_sent_events_show_as_they_arrive", async () => {
    const path = "/sse/3/200";
    const root = serverProject({
      "shipment-ticks.yaml": getRequest("Shipment ticks", path),
    });
    await openRequest(root, "staging", "Shipment ticks");

    // How many events the list shows each time it changes while sending.
    await browser.execute((pane) => {
      const seen: number[] = [];
      (window as unknown as { __deaSeen: number[] }).__deaSeen = seen;
      new MutationObserver(() => {
        if (document.querySelector(pane)?.getAttribute("aria-busy") !== "true")
          return;
        seen.push(
          document.querySelectorAll('ul[aria-label="Server-sent events"] li')
            .length,
        );
      }).observe(document.body, { childList: true, subtree: true });
    }, RESPONSE);

    await sendTo(server, path);
    const list = 'ul[aria-label="Server-sent events"]';
    await browser.waitUntil(
      async () =>
        (await browser.execute(
          (s) => document.querySelectorAll(`${s} li`).length,
          list,
        )) === 3,
      { timeout: 30_000, timeoutMsg: "the list never showed 3 events" },
    );
    const events = await browser.execute(
      (s) =>
        [...document.querySelectorAll(`${s} li`)].map((li) => ({
          time: li.querySelector("time")?.textContent?.trim() ?? "",
          name: li.querySelector(".name")?.textContent?.trim() ?? "",
          data: li.querySelector("code")?.textContent?.trim() ?? "",
        })),
      list,
    );
    expect(events.map((e) => e.name)).toEqual(["tick", "tick", "tick"]);
    expect(events.map((e) => e.data)).toEqual(["1", "2", "3"]);
    const DAY = 24 * 60 * 60 * 1000;
    for (let i = 1; i < events.length; i++) {
      const gap =
        (msOfDay(events[i]?.time ?? "") -
          msOfDay(events[i - 1]?.time ?? "") +
          DAY) %
        DAY;
      expect(gap).toBeGreaterThan(0);
      expect(gap).toBeLessThan(60_000);
    }
    // Some events were in the list before the stream ended.
    const seen = await browser.execute(
      () => (window as unknown as { __deaSeen: number[] }).__deaSeen,
    );
    expect(seen.some((n) => n > 0 && n < 3)).toBe(true);
  });

  it("cookies_are_managed_and_kept_out_of_production", async () => {
    const root = serverProject({
      "set-session.yaml": getRequest("Set session", "/cookies/set", {
        session: "abc",
      }),
      "echo-headers.yaml": getRequest("Echo headers", "/echo"),
    });
    await openRequest(root, "staging", "Set session");
    await sendTo(server, "/cookies/set");

    // The response shows what it set, and leads to the manager.
    await $('[role="tablist"][aria-label="Response sections"]')
      .$("button*=Cookies")
      .click();
    await waitForText('table[aria-label="Received cookies"]', "session");
    await $("button*=Manage cookies").click();
    await browser.waitUntil(async () => (await activeTab()) === "Cookies", {
      timeoutMsg: "the cookie manager did not open",
    });
    await waitForCookie("session", "abc");

    // Edit it: staging sends the new value.
    await $('button[aria-label="Edit cookie session"]').click();
    const dialog = $(
      '//*[@role="dialog"][.//h2[normalize-space(.)="Edit cookie session"]]',
    );
    await dialog.waitForDisplayed();
    const value = dialog.$('.//label[span[normalize-space(.)="Value"]]//input');
    await value.click();
    await browser.keys([MOD, "a"]);
    await typeKeys("xyz");
    expect(await value.getValue()).toBe("xyz");
    await dialog.$("button=Save").click();
    await dialog.waitForExist({ reverse: true });
    await waitForCookie("session", "xyz");

    await showRequest("Echo headers");
    await sendTo(server, "/echo");
    expect(lastCookieHeader()).toBe("session=xyz");

    // Production has a jar of its own.
    await chooseEnvironment("production");
    await sendTo(server, "/echo", true);
    expect(lastCookieHeader()).toBeUndefined();

    // Deleted in staging, it is no longer sent there.
    await chooseEnvironment("staging");
    await tab("Cookies").click();
    await waitForCookie("session", "xyz");
    await $('button[aria-label="Delete cookie session"]').click();
    await browser.waitUntil(
      async () => !(await cookieRows()).some(([name]) => name === "session"),
      { timeout: 30_000, timeoutMsg: "the cookie was not deleted" },
    );
    await showRequest("Echo headers");
    await sendTo(server, "/echo");
    expect(lastCookieHeader()).toBeUndefined();
  });

  it("response_moves_below_and_back", async () => {
    await openRequest(serverProject(), "staging", "Shipment by ID");
    // A work area under 760 px wide always stacks the response below, with
    // no button to move it. At 1280 × 800 the area is about 950 px, but the
    // Windows runner's screen keeps the window near 1028 px, where the
    // sidebar leaves about 700: hide it for this case.
    const splitWidth = () =>
      browser.execute(
        () => document.querySelector(".split")!.getBoundingClientRect().width,
      );
    /** Shows or hides the sidebar with the title bar button, if needed. */
    const sidebar = async (shown: boolean) => {
      const button = $(
        `button[aria-label="${shown ? "Show" : "Hide"} sidebar"]`,
      );
      if (await button.isExisting()) await button.click();
    };
    const wide = async () => {
      await sidebar(false);
      await browser.waitUntil(async () => (await splitWidth()) >= 760, {
        timeoutMsg: "the work area stayed under 760 px",
      });
    };
    const hidSidebar = (await splitWidth()) < 760;
    if (hidSidebar) await wide();
    expect(await splitWidth()).toBeGreaterThanOrEqual(760);
    await sendAndShow();
    // The setting is the app's, so a failure must not leave it below.
    try {
      const right = await waitForLayout("right");
      expect(right.response.left).toBeGreaterThanOrEqual(
        right.editor.right - 1,
      );

      await $('button[aria-label="Show response below"]').click();
      const below = await waitForLayout("below");
      expect(below.response.top).toBeGreaterThanOrEqual(
        below.editor.bottom - 1,
      );

      // Reopened, the tab keeps it. The tree is in the sidebar.
      await $('button[aria-label="Close Shipment by ID"]').click();
      await tab("Shipment by ID").waitForExist({ reverse: true });
      if (hidSidebar) await sidebar(true);
      await showRequest("Shipment by ID");
      if (hidSidebar) await wide();
      await waitForLayout("below");

      await sendAndShow();
      await $('button[aria-label="Show response on the right"]').click();
      await waitForLayout("right");
    } finally {
      // Without the sidebar the area is wide enough to offer the button.
      if (hidSidebar) await sidebar(false);
      const back = $('button[aria-label="Show response on the right"]');
      if (await back.isExisting()) await back.click();
      if (hidSidebar) await sidebar(true);
    }
  });
});
