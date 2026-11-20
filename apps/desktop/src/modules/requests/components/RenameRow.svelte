<!--
  A row's name in a field, drawn over the row. Enter saves, Escape lets go,
  an empty name is refused. One field over the list, rather than one in
  every row, keeps the rows cheap and the field alive when the row scrolls
  out of the rendered part of the list.
-->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import type { TreeRowData } from "../state/tree.svelte";
  import MethodBadge from "./MethodBadge.svelte";

  let {
    row,
    onrename,
    oncancel,
  }: {
    row: TreeRowData;
    onrename: (name: string) => void;
    oncancel: () => void;
  } = $props();

  let invalid = $state(false);
  // A field that was saved or let go ignores the blur of going away.
  let done = false;

  function start(input: HTMLInputElement): void {
    input.focus();
    input.select();
  }

  function save(input: HTMLInputElement): void {
    const name = input.value.trim();
    if (!name) {
      invalid = true;
      return;
    }
    done = true;
    onrename(name);
  }

  function cancel(): void {
    done = true;
    oncancel();
  }

  // The field's keys are its own, not the tree's.
  function onkeydown(e: KeyboardEvent): void {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      save(e.currentTarget as HTMLInputElement);
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
  }

  // Clicking away keeps a name that was typed, and drops an empty one.
  function onblur(e: FocusEvent): void {
    if (done) return;
    const input = e.currentTarget as HTMLInputElement;
    if (input.value.trim()) save(input);
    else cancel();
  }
</script>

<div
  class="rename-row"
  style:padding-left="calc(var(--s-2) + {row.depth * 14}px)"
>
  {#if row.kind === "folder"}<span class="chevron"
      >{#if row.open}<ChevronDown
          size={14}
          strokeWidth={1.5}
        />{:else}<ChevronRight size={14} strokeWidth={1.5} />{/if}</span
    >{:else}<MethodBadge method={row.method ?? ""} size="small" />{/if}<input
    class="field"
    aria-label="Name"
    aria-invalid={invalid || undefined}
    value={row.name}
    autocomplete="off"
    spellcheck="false"
    {@attach start}
    oninput={() => (invalid = false)}
    {onkeydown}
    {onblur}
  />
</div>

<style>
  .rename-row {
    height: 28px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding-right: var(--s-1);
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
  }

  .chevron {
    width: 14px;
    flex: none;
    display: inline-flex;
    color: var(--text-3);
  }

  .field {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 var(--s-1);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-1);
    font: inherit;
  }

  .field:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .field[aria-invalid="true"] {
    box-shadow: inset 0 0 0 1px var(--negative);
  }
</style>
