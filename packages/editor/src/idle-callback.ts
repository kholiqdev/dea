// CodeMirror parses in the background in slices, each in an idle callback.
// A slice that follows the idle deadline lasts at least 25 ms however short
// the deadline, and one without a deadline up to 100 ms. It ends sooner
// only when navigator.scheduling.isInputPending() reports input. WebKit, the
// WebView on macOS and Linux, has no requestIdleCallback, so the stand-in
// below runs each slice from a timer, as likely in the middle of a scroll as
// after it; Chromium, the WebView2 on Windows, waits for idle time. While a
// large body scrolls, WebKitGTK under Xvfb (the Linux runner) takes 25 to
// 40 ms to draw a frame, and a 25 ms slice on top passes the 50 ms a frame
// may take. So every idle callback is timed, and CodeMirror is given an
// isInputPending that ends a slice after 5 ms while the page scrolls and
// after 25 ms otherwise. Slices stay short only while frames count: a
// shorter slice at all times would leave the middle of a 10 MB body
// unhighlighted for many seconds after a jump. CodeMirror looks for both
// functions once, as it loads: import this module first.

/**
 * How long an idle deadline lasts, in milliseconds. CodeMirror reads the
 * deadline only where it finds no isInputPending, and then still parses for
 * 25 ms.
 */
export const IDLE_SLICE_MS = 10;

/** How long a parse slice runs, as CodeMirror's shortest, in milliseconds. */
export const PARSE_SLICE_MS = 25;

/** How long a parse slice runs while the page scrolls, in milliseconds. */
export const SCROLLING_PARSE_SLICE_MS = 5;

/** How long after a scroll event the page still counts as scrolling. */
export const SCROLL_SETTLE_MS = 300;

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

/** When the idle callback running now is to end, or null outside one. */
let sliceEnd: number | null = null;

/** When the page last scrolled. */
let lastScroll = -Infinity;

/** Counts the page as scrolling from now on, for `SCROLL_SETTLE_MS`. */
export function noteScroll(): void {
  lastScroll = Date.now();
}

/**
 * Whether the idle callback running now has had its slice. CodeMirror asks
 * after every step of the parser, millions of times in a large body, so
 * this only compares the clock with the end the callback was given.
 */
export function sliceSpent(): boolean {
  return sliceEnd !== null && Date.now() >= sliceEnd;
}

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
 * Runs `callback` as the idle callback that `sliceSpent` times: 5 ms while
 * the page scrolls, 25 ms otherwise.
 */
function run(callback: IdleRequestCallback, own?: IdleDeadline): void {
  const now = Date.now();
  sliceEnd =
    now +
    (now - lastScroll < SCROLL_SETTLE_MS
      ? SCROLLING_PARSE_SLICE_MS
      : PARSE_SLICE_MS);
  try {
    callback(slice(own));
  } finally {
    sliceEnd = null;
  }
}

/**
 * Gives `host` a requestIdleCallback whose callbacks are timed and whose
 * deadlines last a slice at most: its own one wrapped, or a stand-in when
 * it has none. Only once per host.
 */
export function installIdleCallback(host: IdleHost): void {
  const current = host.requestIdleCallback;
  if (current && INSTALLED in current) return;
  let request: NonNullable<IdleHost["requestIdleCallback"]>;
  if (current) {
    request = (callback, options) =>
      current.call(host, (deadline) => run(callback, deadline), options);
  } else {
    request = (callback) => window.setTimeout(() => run(callback), 1);
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
 * Gives `host` `sliceSpent` as navigator.scheduling.isInputPending, in place
 * of Chromium's own (with which a slice runs 100 ms when nothing is typed)
 * or where there is none (WebKit). Leaves a frozen host as it is.
 */
export function installInputPending(host: SchedulingHost): void {
  try {
    if (host.scheduling) {
      Object.defineProperty(host.scheduling, "isInputPending", {
        value: sliceSpent,
        configurable: true,
        writable: true,
      });
    } else {
      Object.defineProperty(host, "scheduling", {
        value: { isInputPending: sliceSpent },
        configurable: true,
        writable: true,
      });
    }
  } catch {
    // Frozen: the deadlines above still end a slice after 25 ms.
  }
}

if (typeof window !== "undefined") {
  installIdleCallback(window);
  // Scroll events do not bubble, but they pass the window on capture.
  window.addEventListener("scroll", noteScroll, {
    capture: true,
    passive: true,
  });
}
if (typeof navigator !== "undefined")
  installInputPending(navigator as SchedulingHost);
