// Short notices at the bottom of the window, as in the design's
// "Shell: confirmations and toasts".

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  text: string;
  action?: ToastAction;
}

export interface ToastOptions {
  action?: ToastAction;
  /** How long it stays, in milliseconds. */
  timeout?: number;
}

const DEFAULT_TIMEOUT = 6000;

export class Toasts {
  #items = $state.raw<Toast[]>([]);
  #next = 1;

  get items(): Toast[] {
    return this.#items;
  }

  show(text: string, opts: ToastOptions = {}): number {
    const id = this.#next++;
    const toast: Toast = {
      id,
      text,
      ...(opts.action ? { action: opts.action } : {}),
    };
    this.#items = [...this.#items, toast];
    setTimeout(() => this.dismiss(id), opts.timeout ?? DEFAULT_TIMEOUT);
    return id;
  }

  dismiss(id: number): void {
    this.#items = this.#items.filter((t) => t.id !== id);
  }
}

/** The application's toasts. */
export const toasts = new Toasts();
