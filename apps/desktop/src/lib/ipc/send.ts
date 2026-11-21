// Sending a request of this window's project, and the production
// confirmation a send may ask for first.

import { Channel, invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { RequestFile } from "./bindings/RequestFile";
import type { SendAnswer } from "./bindings/SendAnswer";
import type { SendConfirmation } from "./bindings/SendConfirmation";
import type { SendEvent } from "./bindings/SendEvent";
import type { SendOutcome } from "./bindings/SendOutcome";

/** A question whether to send to production, sent to the window that sent. */
export type SendConfirmEvent = { id: number } & SendConfirmation;

/**
 * Sends `request`, the draft of tab `id`, as if saved at `path`, to the
 * window's active environment. `onEvent` hears the progress, starting with
 * `started` just before the request leaves. Rejects only when the window has
 * no readable project; every other ending is a `SendOutcome`.
 */
export function sendRequest(
  id: string,
  path: string,
  request: RequestFile,
  onEvent: (e: SendEvent) => void,
): Promise<SendOutcome> {
  const events = new Channel<SendEvent>(onEvent);
  return invoke<SendOutcome>("send_request", { id, path, request, events });
}

/** Cancels the send of tab `id`, with its confirmation if still open. */
export function cancelSend(id: string): Promise<void> {
  return invoke<void>("cancel_send", { id });
}

/**
 * Calls `handler` for each confirmation of this window. It must listen on
 * this window: Tauri also delivers events sent to another window to a plain
 * `listen`.
 */
export function onSendConfirm(
  handler: (e: SendConfirmEvent) => void,
): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<SendConfirmEvent>(
    "send-confirm",
    (e) => handler(e.payload),
  );
}

/**
 * Calls `handler` with the id of each confirmation of this window that ended
 * without its answer: the send was cancelled, or the page reloaded. Its
 * dialog closes. An id this window never showed may come too.
 */
export function onSendConfirmWithdrawn(
  handler: (id: number) => void,
): Promise<UnlistenFn> {
  return getCurrentWebviewWindow().listen<{ id: number }>(
    "send-confirm-withdrawn",
    (e) => handler(e.payload.id),
  );
}

/** Answers a confirmation once. */
export function answerSendConfirmation(
  id: number,
  answer: SendAnswer,
): Promise<void> {
  return invoke<void>("answer_send_confirmation", { id, answer });
}

/**
 * Sends this window again every confirmation still waiting for it, as
 * `send-confirm`. One sent while the page loaded was lost.
 */
export function resendSendConfirmations(): Promise<void> {
  return invoke<void>("resend_send_confirmations");
}
