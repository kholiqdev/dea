// Short notices at the bottom of the window, as in the design's
// "Shell: confirmations and toasts".

export interface ToastAction {
  label: string;
  run: () => void;
  /** A chord such as `Mod+Z` that runs the action while the toast shows. */
  chord?: string;
}

export interface Toast {
  id: number;
  text: string;
  action?: ToastAction;
  /** Whole seconds left, for a toast with an action. */
  seconds?: number;
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
  #ticks: Record<number, ReturnType<typeof setInterval>> = {};

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
    const timeout = opts.timeout ?? DEFAULT_TIMEOUT;
    if (opts.action) {
      // The countdown shown after the action: whole seconds left.
      toast.seconds = Math.ceil(timeout / 1000);
      const end = Date.now() + timeout;
      this.#ticks[id] = setInterval(() => {
        const seconds = Math.max(0, Math.ceil((end - Date.now()) / 1000));
        this.#items = this.#items.map((t) =>
          t.id === id ? { ...t, seconds } : t,
        );
      }, 1000);
    }
    this.#items = [...this.#items, toast];
    setTimeout(() => this.dismiss(id), timeout);
    return id;
  }

  dismiss(id: number): void {
    clearInterval(this.#ticks[id]);
    delete this.#ticks[id];
    this.#items = this.#items.filter((t) => t.id !== id);
  }
}

/** The application's toasts. */
export const toasts = new Toasts();
