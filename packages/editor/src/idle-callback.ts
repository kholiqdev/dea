// WebKit, the WebView on macOS and Linux, has no requestIdleCallback.
// Without one, CodeMirror parses in the background in 100 ms slices, which
// drops frames while a large body scrolls. This one hands the parser a
// deadline that runs out after a few milliseconds, so it yields often.
// CodeMirror looks for it once, as it loads: import this module first.

/** How long an idle callback may run, in milliseconds. */
export const IDLE_SLICE_MS = 10;

/** Where requestIdleCallback lives: `window`, or a stand-in in tests. */
export interface IdleHost {
  requestIdleCallback?: (
    callback: IdleRequestCallback,
    options?: IdleRequestOptions,
  ) => number;
  cancelIdleCallback?: (handle: number) => void;
}

/** Gives `host` a requestIdleCallback when it has none. */
export function installIdleCallback(host: IdleHost): void {
  if (host.requestIdleCallback !== undefined) return;
  host.requestIdleCallback = (callback) =>
    window.setTimeout(() => {
      const start = performance.now();
      callback({
        didTimeout: false,
        timeRemaining: () =>
          Math.max(0, IDLE_SLICE_MS - (performance.now() - start)),
      });
    }, 1);
  host.cancelIdleCallback = (handle) => window.clearTimeout(handle);
}

if (typeof window !== "undefined") installIdleCallback(window);
