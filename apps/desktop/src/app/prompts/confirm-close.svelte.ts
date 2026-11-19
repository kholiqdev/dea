// The question asked before closing a tab with unsaved changes.

import type { CloseChoice, TabRecord } from "../tabs/tabs.svelte";

interface Pending {
  tab: TabRecord;
  answer: (choice: CloseChoice) => void;
}

export class CloseQuestion {
  #pending = $state.raw<Pending | null>(null);

  get pending(): Pending | null {
    return this.#pending;
  }

  /** Shows the question; a second one waits for the first. */
  ask = (tab: TabRecord): Promise<CloseChoice> => {
    this.#pending?.answer("cancel");
    return new Promise((resolve) => {
      this.#pending = {
        tab,
        answer: (choice) => {
          this.#pending = null;
          resolve(choice);
        },
      };
    });
  };
}

/** The application's close question. */
export const closeQuestion = new CloseQuestion();
