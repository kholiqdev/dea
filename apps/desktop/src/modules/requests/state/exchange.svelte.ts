// The send of each request tab and what came back of it, by tab id. They
// live in the module, like the drafts, so a tab that is not shown keeps its
// response and a send keeps running.

import { projectApi } from "$app/contributions/project-api";
import { shellApi } from "$app/contributions/shell-api";
import type { HeaderRow } from "$lib/ipc/bindings/HeaderRow";
import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
import type { SendEvent } from "$lib/ipc/bindings/SendEvent";
import type { SendFailure } from "$lib/ipc/bindings/SendFailure";
import type { SendOutcome } from "$lib/ipc/bindings/SendOutcome";
import { errorMessage } from "$lib/ipc/errors";
import { cancelSend, releaseResponse, sendRequest } from "../api";
import type { RequestDraft } from "./drafts.svelte";

/** A connection phase as it ended; `reused` took no time. */
export type SendStep = {
  kind: "dns" | "connect" | "tls" | "reused";
  ms: number;
};

/** One server-sent event, timed from the start of the request. */
export type SseRow = {
  atMs: number;
  event: string | null;
  data: string;
  id: string | null;
};

export type HeadInfo = { status: number; reason: string; headers: HeaderRow[] };

/**
 * What a tab shows of its send. While `sending`, `startedAt` is the
 * `performance.now()` at which the request left (or the send began, before
 * that), `host` is known once it left, and `text` is the body so far while
 * Dea still forwards it; `received` counts every byte. `environment` is
 * the one the request was sent in.
 */
export type ExchangeState =
  | { phase: "idle" }
  | {
      phase: "sending";
      startedAt: number;
      host: string | null;
      steps: SendStep[];
      head: HeadInfo | null;
      received: number;
      text: string;
      textBytes: number;
      events: SseRow[];
      droppedEvents: number;
    }
  | {
      phase: "done";
      response: ResponseInfo;
      environment: string | null;
      startedAt: number;
      events: SseRow[];
      droppedEvents: number;
    }
  | { phase: "failed"; failure: SendFailure; environment: string | null }
  | {
      phase: "cancelled";
      afterMs: number;
      startedAt: number;
      head: HeadInfo | null;
      events: SseRow[];
      droppedEvents: number;
    };

/** Events kept of a stream; the older ones are dropped. */
export const MAX_EVENTS = 10_000;

const ENCODER = new TextEncoder();

const PHASES = ["dns", "connect", "tls"] as const;

function isPhase(phase: string): phase is (typeof PHASES)[number] {
  return (PHASES as readonly string[]).includes(phase);
}

/** The `host:port` of a URL as Dea sent it; null when it does not parse. */
function hostOf(url: string): string | null {
  try {
    return new URL(url).host || null;
  } catch {
    return null;
  }
}

/**
 * How many times a send looks at a draft that keeps changing under it
 * before it goes by the latest look.
 */
const MAX_LOOKS = 3;

let sendsEnded = $state(0);

/**
 * Goes up each time a send ends, apart from one not confirmed: a response
 * may have set cookies.
 */
export function cookiesVersion(): number {
  return sendsEnded;
}

/** The sends of one request tab. */
export class Exchange {
  state = $state<ExchangeState>({ phase: "idle" });
  /** The undefined variables the last send stopped for; null when it did not. */
  warning = $state.raw<string[] | null>(null);

  readonly #id: string;
  // From the look at the draft until the outcome: a second send is ignored.
  #busy = false;
  #disposed = false;
  // The sends so far, and which one the state shows: progress of another
  // send is dropped.
  #sends = 0;
  #shown = 0;

  constructor(tabId: string) {
    this.#id = tabId;
  }

  /**
   * Sends the draft as it is in the editor, saved or not. It stops at a
   * warning when the draft uses variables nothing defines, unless sent
   * `anyway`.
   */
  async send(
    draft: RequestDraft,
    opts: { anyway?: boolean } = {},
  ): Promise<void> {
    if (this.#busy || this.#disposed) return;
    this.#busy = true;
    try {
      // The warning goes by what this very draft uses. The tab's own look,
      // still waiting, would only replace this one with an answer that
      // comes later, so it is called off.
      for (let look = 0; look < MAX_LOOKS && !draft.inspectionCurrent; look++) {
        draft.cancelInspection();
        await draft.inspect();
        if (this.#disposed) return;
      }
      const names = draft.inspection?.undefined ?? [];
      if (!opts.anyway && names.length > 0) {
        this.warning = [...names];
        return;
      }
      this.warning = null;
      await this.#send(draft);
    } finally {
      this.#busy = false;
    }
  }

  async #send(draft: RequestDraft): Promise<void> {
    const before = this.state;
    const shownBefore = this.#shown;
    const call = ++this.#sends;
    const environment = projectApi().environment?.name ?? null;
    this.state = {
      phase: "sending",
      startedAt: performance.now(),
      host: null,
      steps: [],
      head: null,
      received: 0,
      text: "",
      textBytes: 0,
      events: [],
      droppedEvents: 0,
    };
    this.#shown = call;
    // What #hear fills in; only this send changes the state while it runs.
    const sent = this.state;
    let outcome: SendOutcome;
    try {
      outcome = await sendRequest(this.#id, draft.path, draft.request(), (e) =>
        this.#hear(e, call),
      );
    } catch (e) {
      // The window has no project to send from: nothing left.
      this.state = before;
      this.#shown = shownBefore;
      shellApi().toast(errorMessage(e));
      return;
    }
    switch (outcome.type) {
      case "notConfirmed":
        this.state = before;
        this.#shown = shownBefore;
        return;
      case "response": {
        const { response } = outcome;
        this.state = {
          phase: "done",
          response,
          environment,
          startedAt: sent.startedAt,
          events: sent.events,
          droppedEvents: sent.droppedEvents,
        };
        if (response.cookiesNotSaved !== null)
          shellApi().toast(
            `Cookies from this response were not saved: ${response.cookiesNotSaved}`,
          );
        break;
      }
      case "failed":
        this.state = { phase: "failed", failure: outcome.failure, environment };
        break;
      case "cancelled":
        this.state = {
          phase: "cancelled",
          afterMs: performance.now() - sent.startedAt,
          startedAt: sent.startedAt,
          head: sent.head,
          events: sent.events,
          droppedEvents: sent.droppedEvents,
        };
        break;
    }
    // A new response took the old one's place; without one, it goes too.
    if (outcome.type !== "response" && before.phase === "done")
      void releaseResponse(this.#id);
    sendsEnded++;
  }

  /**
   * Takes in the progress of send `call`. A large message comes another way
   * than a small one and can land after the outcome: a stream event then
   * joins the finished send. Its body is read from Dea once it is done, so
   * late text has nothing to add.
   */
  #hear(e: SendEvent, call: number): void {
    if (call !== this.#shown) return;
    const s = this.state;
    if (s.phase === "done" || s.phase === "cancelled") {
      if (e.type === "event") keepEvent(s, e);
      return;
    }
    if (s.phase !== "sending") return;
    switch (e.type) {
      case "started":
        s.startedAt = performance.now();
        s.host = hostOf(e.url);
        break;
      case "phase":
        if (isPhase(e.phase)) s.steps.push({ kind: e.phase, ms: e.ms });
        break;
      case "reused":
        s.steps.push({ kind: "reused", ms: 0 });
        break;
      case "head":
        s.head = { status: e.status, reason: e.reason, headers: e.headers };
        break;
      case "body":
        s.received = e.received;
        if (e.text !== null) {
          s.text += e.text;
          s.textBytes += ENCODER.encode(e.text).length;
        }
        break;
      case "event":
        keepEvent(s, e);
        break;
    }
  }

  /** Stops the send under way; its outcome comes back `cancelled`. */
  cancel(): void {
    if (this.state.phase === "sending") void cancelSend(this.#id);
  }

  /** The tab closed: its send stops, and its response is let go. */
  dispose(): void {
    this.#disposed = true;
    if (this.state.phase === "sending") void cancelSend(this.#id);
    void releaseResponse(this.#id);
  }
}

/** Adds a server-sent event; an endless stream keeps its last ones only. */
function keepEvent(
  s: { events: SseRow[]; droppedEvents: number },
  e: Extract<SendEvent, { type: "event" }>,
): void {
  s.events.push({ atMs: e.atMs, event: e.event, data: e.data, id: e.id });
  if (s.events.length > MAX_EVENTS) {
    s.events.shift();
    s.droppedEvents++;
  }
}

/** The exchanges of the request tabs sent from so far, by tab id. */
// Only looked up by id, never watched, so a plain Map is right here.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
export const exchanges = new Map<string, Exchange>();

/** The exchange of request tab `tabId`, made the first time it is asked for. */
export function exchangeFor(tabId: string): Exchange {
  let exchange = exchanges.get(tabId);
  if (!exchange) {
    exchange = new Exchange(tabId);
    exchanges.set(tabId, exchange);
  }
  return exchange;
}

/** Ends the exchange of a closed tab. */
export function releaseExchange(tabId: string): void {
  exchanges.get(tabId)?.dispose();
  exchanges.delete(tabId);
}

/** Forgets every exchange without telling Dea (tests). */
export function resetExchanges(): void {
  exchanges.clear();
}
