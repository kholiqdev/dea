import { afterEach, expect, test, vi } from "vitest";
import editorSource from "./CodeEditor.svelte?raw";
import {
  IDLE_SLICE_MS,
  PARSE_SLICE_MS,
  SCROLL_SETTLE_MS,
  SCROLLING_PARSE_SLICE_MS,
  installIdleCallback,
  installInputPending,
  noteScroll,
  sliceSpent,
  type IdleHost,
} from "./idle-callback";

afterEach(() => vi.restoreAllMocks());

/** A host given the shim, with its two functions. */
function shimmed(): Required<IdleHost> {
  const host: IdleHost = {};
  installIdleCallback(host);
  const { requestIdleCallback, cancelIdleCallback } = host;
  if (!requestIdleCallback || !cancelIdleCallback)
    throw new Error("nothing was installed");
  return { requestIdleCallback, cancelIdleCallback };
}

/**
 * A host with its own requestIdleCallback, like Chromium's, whose
 * deadlines report 50 ms left and which runs callbacks when told to.
 */
function native(remaining = 50) {
  const pending: IdleRequestCallback[] = [];
  const own = vi.fn<NonNullable<IdleHost["requestIdleCallback"]>>((callback) =>
    pending.push(callback),
  );
  const cancel = vi.fn();
  const host: IdleHost = {
    requestIdleCallback: own,
    cancelIdleCallback: cancel,
  };
  const run = () => {
    for (const callback of pending.splice(0))
      callback({ didTimeout: false, timeRemaining: () => remaining });
  };
  return { host, own, cancel, run };
}

test("a_host_that_has_one_keeps_it_with_deadlines_cut_to_a_slice", () => {
  vi.spyOn(performance, "now").mockReturnValue(100);
  const { host, own, cancel, run } = native();
  installIdleCallback(host);
  const seen: number[] = [];
  const handle = host.requestIdleCallback?.(
    (deadline) => seen.push(deadline.timeRemaining()),
    { timeout: 400 },
  );
  expect(own).toHaveBeenCalledWith(expect.any(Function), { timeout: 400 });
  expect(handle).toBe(1);
  run();
  expect(seen).toEqual([IDLE_SLICE_MS]);
  expect(host.cancelIdleCallback).toBe(cancel);
});

test("a_cut_deadline_runs_out_with_its_slice_or_before", () => {
  let now = 100;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  const deadlines: IdleDeadline[] = [];
  const long = native(50);
  installIdleCallback(long.host);
  long.host.requestIdleCallback?.((d) => deadlines.push(d));
  const short = native(3);
  installIdleCallback(short.host);
  short.host.requestIdleCallback?.((d) => deadlines.push(d));
  long.run();
  short.run();
  const [fromLong, fromShort] = deadlines;
  now += 4;
  expect(fromLong?.timeRemaining()).toBe(IDLE_SLICE_MS - 4);
  expect(fromShort?.timeRemaining()).toBe(3);
  now += 30;
  expect(fromLong?.timeRemaining()).toBe(0);
});

test("installing_twice_wraps_once", () => {
  const { host, own, run } = native();
  installIdleCallback(host);
  const wrapped = host.requestIdleCallback;
  installIdleCallback(host);
  expect(host.requestIdleCallback).toBe(wrapped);
  const ran = vi.fn();
  host.requestIdleCallback?.(ran);
  expect(own).toHaveBeenCalledTimes(1);
  run();
  expect(ran).toHaveBeenCalledTimes(1);
});

test("the_shim_is_installed_once_too", () => {
  const host: IdleHost = {};
  installIdleCallback(host);
  const shim = host.requestIdleCallback;
  installIdleCallback(host);
  expect(host.requestIdleCallback).toBe(shim);
});

test("a_host_without_one_gets_slices_that_run_out", async () => {
  let now = 100;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  const host = shimmed();
  const deadline = await new Promise<IdleDeadline>((resolve) =>
    host.requestIdleCallback(resolve),
  );
  expect(deadline.didTimeout).toBe(false);
  expect(deadline.timeRemaining()).toBe(IDLE_SLICE_MS);
  now += 4;
  expect(deadline.timeRemaining()).toBe(IDLE_SLICE_MS - 4);
  now += 30;
  expect(deadline.timeRemaining()).toBe(0);
});

test("a_cancelled_callback_never_runs", async () => {
  const host = shimmed();
  const ran = vi.fn();
  host.cancelIdleCallback(host.requestIdleCallback(ran));
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(ran).not.toHaveBeenCalled();
});

test("the_editor_installs_it_before_codemirror_looks_for_it", () => {
  // @codemirror/language checks for requestIdleCallback once, as it loads.
  const shim = editorSource.indexOf('import "./idle-callback";');
  const language = editorSource.indexOf('from "@codemirror/language"');
  expect(shim).toBeGreaterThan(-1);
  expect(editorSource.indexOf("import ")).toBe(shim);
  expect(shim).toBeLessThan(language);
});

/**
 * The clock `sliceSpent` reads, standing at `now` until moved. It starts
 * past any scroll an earlier test noted.
 */
function clock() {
  const c = { now: Date.now() + 1e9 };
  vi.spyOn(Date, "now").mockImplementation(() => c.now);
  return c;
}

/** Whether a callback is spent after `ms - 1` and after `ms`, in order. */
function spentAround(c: { now: number }, ms: number): () => boolean[] {
  const seen: boolean[] = [];
  return () => {
    seen.push(sliceSpent());
    c.now += ms - 1;
    seen.push(sliceSpent());
    c.now += 1;
    seen.push(sliceSpent());
    return seen;
  };
}

test("an_idle_callback_is_spent_after_25_ms_and_only_while_it_runs", async () => {
  const c = clock();
  const { host, run } = native();
  installIdleCallback(host);
  let seen: boolean[] = [];
  const check = spentAround(c, PARSE_SLICE_MS);
  host.requestIdleCallback?.(() => (seen = check()));
  run();
  expect(seen).toEqual([false, false, true]);
  expect(sliceSpent()).toBe(false);
  await new Promise<void>((resolve) =>
    shimmed().requestIdleCallback(() => {
      seen = spentAround(c, PARSE_SLICE_MS)();
      resolve();
    }),
  );
  expect(seen).toEqual([false, false, true]);
});

test("while_the_page_scrolls_a_callback_is_spent_after_5_ms", () => {
  const c = clock();
  const { host, run } = native();
  installIdleCallback(host);
  const runOne = (ms: number) => {
    let seen: boolean[] = [];
    host.requestIdleCallback?.(() => (seen = spentAround(c, ms)()));
    run();
    return seen;
  };
  // Scroll events do not bubble: the window hears them on capture.
  const scroller = document.body.appendChild(document.createElement("div"));
  scroller.dispatchEvent(new Event("scroll"));
  scroller.remove();
  expect(runOne(SCROLLING_PARSE_SLICE_MS)).toEqual([false, false, true]);
  // Still scrolling 1 ms before it settles, and settled from then on.
  noteScroll();
  c.now += SCROLL_SETTLE_MS - 1;
  expect(runOne(SCROLLING_PARSE_SLICE_MS)).toEqual([false, false, true]);
  expect(runOne(PARSE_SLICE_MS)).toEqual([false, false, true]);
});

test("gives_codemirror_an_isInputPending_with_or_without_one_and_copes_when_frozen", () => {
  const chromium = { scheduling: { isInputPending: () => false } };
  installInputPending(chromium);
  expect(chromium.scheduling.isInputPending).toBe(sliceSpent);
  const webkit: { scheduling?: { isInputPending?: unknown } } = {};
  installInputPending(webkit);
  expect(webkit.scheduling?.isInputPending).toBe(sliceSpent);
  expect(() => installInputPending(Object.freeze({}))).not.toThrow();
  expect(() =>
    installInputPending(
      Object.freeze({ scheduling: Object.freeze({ isInputPending: 1 }) }),
    ),
  ).not.toThrow();
});

test("codemirror_ends_its_parse_slices_after_5_ms_while_scrolling_and_25_after", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const [{ EditorState }, { EditorView }, language, { json }] =
      await Promise.all([
        import("@codemirror/state"),
        import("@codemirror/view"),
        import("@codemirror/language"),
        import("@codemirror/lang-json"),
      ]);
    const rows = Array.from({ length: 50_000 }, (_, i) => `{"i":${i}}`);
    const view = new EditorView({
      state: EditorState.create({
        doc: `[${rows.join(",\n")}]`,
        extensions: [json()],
      }),
      parent: document.body,
    });
    // Every reading of the clock moves it 1 ms on, so a slice lasts as
    // many milliseconds as it reads the clock.
    const c = { now: Date.now() + 2e9 };
    vi.spyOn(Date, "now").mockImplementation(() => c.now++);
    const slice = async () => {
      const start = c.now;
      // CodeMirror waits 100 ms before it asks for an idle callback.
      await vi.advanceTimersByTimeAsync(101);
      return c.now - start;
    };
    const before = language.syntaxTree(view.state).length;
    noteScroll();
    const scrolling = await slice();
    c.now += SCROLL_SETTLE_MS;
    const settled = await slice();
    // Run to its end, the first slice would have parsed 100 000 characters on.
    expect(language.syntaxTree(view.state).length).toBe(before);
    expect(language.syntaxParserRunning(view)).toBe(true);
    expect(scrolling).toBeLessThan(2 * SCROLLING_PARSE_SLICE_MS);
    expect(settled).toBeGreaterThanOrEqual(PARSE_SLICE_MS);
    expect(settled).toBeLessThan(PARSE_SLICE_MS + 10);
    view.destroy();
  } finally {
    vi.useRealTimers();
  }
});
