// Fakes the window's `send-confirm` events and records the answers sent back.

import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { SendAnswer } from "$lib/ipc/bindings/SendAnswer";
import type { SendConfirmEvent } from "$lib/ipc/send";
import { sendConfirmations } from "./send-confirm.svelte";

let confirm: (e: SendConfirmEvent) => void = () => {};
let withdraw: (id: number) => void = () => {};
let stop: () => void = () => {};

/**
 * Starts the shell's confirmations on a fake window, answered into the
 * returned list of `[id, answer]`, then delivers `event` when given.
 */
export function fakeSendConfirm(
  event?: SendConfirmEvent,
): [number, SendAnswer][] {
  const answers: [number, SendAnswer][] = [];
  clearMocks();
  mockIPC((cmd, args) => {
    if (cmd === "answer_send_confirmation") {
      const a = args as { id: number; answer: SendAnswer };
      answers.push([a.id, a.answer]);
    }
    return null;
  });
  // Nothing of an earlier test stays open.
  stop();
  while (sendConfirmations.current)
    sendConfirmations.withdraw(sendConfirmations.current.id);
  void sendConfirmations
    .listen(
      (handler) => {
        confirm = handler;
        return Promise.resolve(() => {});
      },
      () => Promise.resolve(),
      (handler) => {
        withdraw = handler;
        return Promise.resolve(() => {});
      },
    )
    .then((unlisten) => {
      stop = unlisten;
    });
  if (event) emitSendConfirm(event);
  return answers;
}

export function emitSendConfirm(event: SendConfirmEvent): void {
  confirm(event);
}

export function emitSendConfirmWithdrawn(id: number): void {
  withdraw(id);
}
