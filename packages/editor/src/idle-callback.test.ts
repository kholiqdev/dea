import { afterEach, expect, test, vi } from "vitest";
import editorSource from "./CodeEditor.svelte?raw";
import {
  IDLE_SLICE_MS,
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

test("a_host_that_has_one_keeps_its_own", () => {
  const own = vi.fn(() => 7);
  const cancel = vi.fn();
  const host: IdleHost = {
    requestIdleCallback: own,
    cancelIdleCallback: cancel,
  };
  installIdleCallback(host);
  expect(host.requestIdleCallback).toBe(own);
  expect(host.cancelIdleCallback).toBe(cancel);
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
