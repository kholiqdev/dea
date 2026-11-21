<!--
  A button that opens a menu. Arrows, Home and End move between items,
  Enter or Space chooses, Escape and Tab close it; focus goes back to the
  button. Items with `checked` are radio items.
-->
<script module lang="ts">
  export interface MenuItem {
    id: string;
    label: string;
    /** Makes the item a radio item, checked or not. */
    checked?: boolean;
    disabled?: boolean;
    /** A CSS colour for a dot before the label. */
    dot?: string;
    /** A short word after the label, such as "production". */
    badge?: string;
    /** A shortcut shown at the end, such as "⌘E". */
    shortcut?: string;
    /** Draws a line before this item. */
    separatorBefore?: boolean;
    /** "danger" colours the label red. */
    tone?: "danger";
    onselect: () => void;
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open = $bindable(false),
    items,
    menuLabel,
    title,
    trigger,
  }: {
    open?: boolean;
    items: MenuItem[];
    /** The menu's accessible name. */
    menuLabel: string;
    title?: string;
    trigger: Snippet;
  } = $props();

  let root: HTMLElement | undefined;
  let button: HTMLButtonElement | undefined;

  function entries(menu: Element): HTMLElement[] {
    return [
      ...menu.querySelectorAll<HTMLElement>(
        "[role^='menuitem']:not([disabled])",
      ),
    ];
  }

  function close(focusButton: boolean): void {
    open = false;
    if (focusButton) button?.focus();
  }

  function focusStart(menu: HTMLElement): void {
    const list = entries(menu);
    (
      list.find((el) => el.getAttribute("aria-checked") === "true") ?? list[0]
    )?.focus();
  }

  function choose(item: MenuItem): void {
    close(true);
    item.onselect();
  }

  function onkeydown(e: KeyboardEvent): void {
    const menu = e.currentTarget as HTMLElement;
    const list = entries(menu);
    const at = list.indexOf(document.activeElement as HTMLElement);
    const to: Record<string, number> = {
      ArrowDown: (at + 1) % list.length,
      ArrowUp: (at - 1 + list.length) % list.length,
      Home: 0,
      End: list.length - 1,
    };
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(true);
    } else if (e.key === "Tab") {
      close(false);
    } else if (e.key in to) {
      e.preventDefault();
      list[to[e.key] ?? 0]?.focus();
    }
  }

  function onpointerdown(e: PointerEvent): void {
    if (open && root && !root.contains(e.target as Node)) close(false);
  }
</script>

<svelte:window {onpointerdown} />

<div class="menu-root" bind:this={root}>
  <button
    bind:this={button}
    type="button"
    class="trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    {title}
    onclick={() => (open = !open)}
  >
    {@render trigger()}
  </button>
  {#if open}
    <div
      class="menu"
      role="menu"
      aria-label={menuLabel}
      tabindex="-1"
      {onkeydown}
      {@attach focusStart}
    >
      {#each items as item (item.id)}
        {#if item.separatorBefore}<div
            class="separator"
            role="separator"
          ></div>{/if}
        <button
          type="button"
          class="item"
          data-tone={item.tone}
          role={item.checked === undefined ? "menuitem" : "menuitemradio"}
          aria-checked={item.checked}
          disabled={item.disabled}
          tabindex="-1"
          onclick={() => choose(item)}
        >
          {#if item.dot}<span class="dot" style:background={item.dot}
            ></span>{/if}
          <span class="label">{item.label}</span>
          {#if item.badge}<span class="badge">{item.badge}</span>{/if}
          {#if item.shortcut}<span class="shortcut">{item.shortcut}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu-root {
    position: relative;
    display: inline-flex;
  }

  .trigger {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
    white-space: nowrap;
  }

  .trigger:hover,
  .trigger[aria-expanded="true"] {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .menu {
    position: absolute;
    top: calc(100% + var(--s-1));
    left: 0;
    z-index: 40;
    min-width: 220px;
    display: flex;
    flex-direction: column;
    padding: var(--s-1);
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow:
      inset 0 0 0 1px var(--line-strong),
      var(--shadow-float);
  }

  .menu:focus {
    outline: none;
  }

  .item {
    height: 30px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-1);
    text-align: left;
    white-space: nowrap;
  }

  .item:hover,
  .item:focus-visible {
    background: var(--bg-hover);
  }

  .item[aria-checked="true"] {
    background: var(--bg-3);
  }

  .item[data-tone="danger"] {
    color: var(--negative-text);
  }

  .separator {
    height: 1px;
    margin: var(--s-1) var(--s-2);
    background: var(--line-strong);
  }

  .item:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .label {
    flex: 1;
  }

  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 4px;
  }

  .badge {
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 400 11px/1 var(--font-mono);
    color: var(--negative-text);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .shortcut {
    font: 500 11px/1 var(--font-mono);
    color: var(--text-3);
  }
</style>
