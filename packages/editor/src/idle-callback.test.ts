import { afterEach, expect, test, vi } from "vitest";
import editorSource from "./CodeEditor.svelte?raw";
import {
  IDLE_SLICE_MS,
  hideInputPending,
  installIdleCallback,
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

test("hides isInputPending from CodeMirror, once, and copes without scheduling", () => {
  const navigator = {
    scheduling: { isInputPending: () => false } as {
      isInputPending?: () => boolean;
    },
  };
  hideInputPending(navigator);
  expect(navigator.scheduling?.isInputPending).toBeFalsy();
  hideInputPending(navigator);
  expect(navigator.scheduling?.isInputPending).toBeFalsy();
  expect(() => hideInputPending({})).not.toThrow();
  expect(() => hideInputPending({ scheduling: undefined })).not.toThrow();
});
