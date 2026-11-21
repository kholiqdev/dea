// The question whether to send to production: one at a time, each answered once.

import { errorMessage } from "$lib/ipc/errors";
import * as ipc from "$lib/ipc/send";
import type { SendAnswer } from "$lib/ipc/bindings/SendAnswer";
import type { SendConfirmEvent } from "$lib/ipc/send";
import { toasts } from "./toasts.svelte";

export class SendConfirmations {
  #queue = $state.raw<SendConfirmEvent[]>([]);

  /** The confirmation shown now: the oldest one not yet answered. */
  get current(): SendConfirmEvent | null {
    return this.#queue[0] ?? null;
  }

  /** Queues a confirmation; one asked again (after a reload) is kept once. */
  show(confirmation: SendConfirmEvent): void {
    if (this.#queue.some((c) => c.id === confirmation.id)) return;
    this.#queue = [...this.#queue, confirmation];
  }

  /** Drops a confirmation that ended without an answer, if this one has it. */
  withdraw(id: number): void {
    if (!this.#queue.some((c) => c.id === id)) return;
    this.#queue = this.#queue.filter((c) => c.id !== id);
  }

  /** Answers the confirmation shown now; the next one comes up. */
  answer(answer: SendAnswer): void {
    const confirmation = this.current;
    if (!confirmation) return;
    this.#queue = this.#queue.slice(1);
    ipc
      .answerSendConfirmation(confirmation.id, answer)
      .catch((e: unknown) => toasts.show(errorMessage(e)));
  }

  /**
   * Shows the confirmations of this window until the returned function is
   * called. Once listening, asks for those sent while the page loaded: they
   * were lost, and the send waits for their answer.
   */
  async listen(
    subscribe = ipc.onSendConfirm,
    resend = ipc.resendSendConfirmations,
    subscribeWithdrawn = ipc.onSendConfirmWithdrawn,
  ): Promise<() => void> {
    const stopShowing = await subscribe((c) => this.show(c));
    const stopWithdrawing = await subscribeWithdrawn((id) => this.withdraw(id));
    await resend();
    return () => {
      stopShowing();
      stopWithdrawing();
    };
  }
}

/** The application's confirmations to send to production. */
export const sendConfirmations = new SendConfirmations();
