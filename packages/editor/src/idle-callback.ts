// CodeMirror parses in the background for as long as an idle deadline
// allows, at least 25 ms and at most 100. WebKit, the WebView on macOS and
// Linux, has no requestIdleCallback, so it would take 100 ms slices; the
// deadlines of Chromium, the WebView2 on Windows, reach 50 ms. Either drops
// frames while a large body scrolls. So the parser gets deadlines that run
// out after a few milliseconds: a stand-in where there is no
// requestIdleCallback, and the browser's own with its deadlines cut short
// where there is. CodeMirror looks for it once, as it loads: import this
// module first.

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

/** Marks a requestIdleCallback installed here, even by another copy. */
const INSTALLED = Symbol.for("dea.editor.idle-callback");

/** A deadline that runs out a slice after now, or with `own` if sooner. */
function slice(own?: IdleDeadline): IdleDeadline {
  const start = performance.now();
  const left = () => Math.max(0, IDLE_SLICE_MS - (performance.now() - start));
  return {
    didTimeout: own?.didTimeout ?? false,
    timeRemaining: () => (own ? Math.min(own.timeRemaining(), left()) : left()),
  };
}

/**
 * Gives `host` a requestIdleCallback whose deadlines last a slice at most:
 * its own one wrapped, or a stand-in when it has none. Only once per host.
 */
export function installIdleCallback(host: IdleHost): void {
  const current = host.requestIdleCallback;
  if (current && INSTALLED in current) return;
  let request: NonNullable<IdleHost["requestIdleCallback"]>;
  if (current) {
    request = (callback, options) =>
      current.call(host, (deadline) => callback(slice(deadline)), options);
  } else {
    request = (callback) => window.setTimeout(() => callback(slice()), 1);
    host.cancelIdleCallback = (handle) => window.clearTimeout(handle);
  }
  host.requestIdleCallback = Object.assign(request, {
    [INSTALLED]: true as const,
  });
}

/** Where navigator.scheduling lives: `navigator`, or a stand-in in tests. */
export interface SchedulingHost {
  scheduling?: { isInputPending?: unknown };
}

/**
 * Chromium, the WebView2 on Windows, has navigator.scheduling.isInputPending,
 * and CodeMirror reads it as it loads. With it, a parse slice ignores the
 * idle deadline and runs up to 100 ms, stopping only for input; a scroll that
 * a script or a smooth animation drives is not input, so frames drop. Without
 * it CodeMirror takes the deadline, which the shim above keeps short. So
 * hide it, as WebKit has no such function either.
 */
export function hideInputPending(host: SchedulingHost): void {
  try {
    const scheduling = host.scheduling;
    if (!scheduling || scheduling.isInputPending === undefined) return;
    Object.defineProperty(scheduling, "isInputPending", {
      value: undefined,
      configurable: true,
      writable: true,
    });
  } catch {
    // A frozen scheduling object: leave it as it is.
  }
}

if (typeof window !== "undefined") installIdleCallback(window);
if (typeof navigator !== "undefined")
  hideInputPending(navigator as SchedulingHost);
