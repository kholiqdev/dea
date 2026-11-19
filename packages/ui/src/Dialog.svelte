<!--
  A modal dialog. Focus moves into it and stays there, Escape cancels, and
  focus goes back to whatever had it before. Mark the control that should
  get focus first with `data-autofocus`.
-->
<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  let {
    title,
    oncancel,
    children,
    actions,
  }: {
    title: string;
    oncancel: () => void;
    children?: Snippet;
    actions?: Snippet;
  } = $props();

  const id = `dialog-${Math.random().toString(36).slice(2)}`;
  let box: HTMLElement;

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function focusables(): HTMLElement[] {
    return [...box.querySelectorAll<HTMLElement>(FOCUSABLE)];
  }

  onMount(() => {
    const before = document.activeElement as HTMLElement | null;
    const first =
      box.querySelector<HTMLElement>("[data-autofocus]") ??
      focusables()[0] ??
      box;
    first.focus();
    return () => {
      if (before?.isConnected) before.focus();
    };
  });

  function onkeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      oncancel();
      return;
    }
    if (e.key !== "Tab") return;
    const items = focusables();
    const first = items[0];
    const last = items.at(-1);
    if (!first || !last) {
      e.preventDefault();
      return;
    }
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<div class="scrim"></div>
<div
  bind:this={box}
  class="dialog"
  role="dialog"
  aria-modal="true"
  aria-labelledby={id}
  tabindex="-1"
  {onkeydown}
>
  <h2 {id}>{title}</h2>
  {@render children?.()}
  {#if actions}
    <div class="actions">{@render actions()}</div>
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--scrim);
  }

  .dialog {
    position: fixed;
    z-index: 60;
    top: 20vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, calc(100vw - 2 * var(--s-6)));
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    padding: var(--s-5);
    border-radius: var(--r-panel);
    background: var(--bg-2);
    box-shadow:
      inset 0 0 0 1px var(--line-strong),
      var(--shadow-float);
    color: var(--text-1);
  }

  .dialog:focus {
    outline: none;
  }

  h2 {
    font-size: 15px;
    line-height: 20px;
    font-weight: 500;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--s-2);
  }
</style>
