<!--
  A one-line field that shows {{variables}} as chips. It is a plain <input>
  with its text made invisible; a layer over it draws the same text with the
  chips, in the same font and padding and shifted by the input's own scroll.
  The layer lets pointer events through except on a chip, which is for hover.
  A variable's value is shown while the mouse is on its chip or the text
  cursor is inside it.
-->
<script lang="ts">
  import { variableRanges } from "@dea/editor";
  import { onMount } from "svelte";
  import type { VariableLookup } from "../state/variables.svelte";
  import VariablePopover from "./VariablePopover.svelte";

  let {
    value,
    label,
    placeholder,
    mono = false,
    variables,
    oninput,
    onenter,
  }: {
    value: string;
    label: string;
    placeholder?: string;
    mono?: boolean;
    variables: VariableLookup;
    oninput: (value: string) => void;
    onenter?: () => void;
  } = $props();

  const uid = $props.id();
  const popoverId = `variable-popover-${uid}`;

  let input: HTMLInputElement;
  let scroll = $state(0);
  let caret = $state<number | null>(null);
  let hovered = $state<number | null>(null);
  // The chip whose popover Escape closed, until the cursor or mouse moves on.
  let dismissed = $state<number | null>(null);

  type Piece = { text: string; variable?: number };
  const ranges = $derived(variableRanges(value));
  const pieces = $derived.by(() => {
    const out: Piece[] = [];
    let at = 0;
    ranges.forEach((r, i) => {
      if (r.from > at) out.push({ text: value.slice(at, r.from) });
      out.push({ text: value.slice(r.from, r.to), variable: i });
      at = r.to;
    });
    if (at < value.length) out.push({ text: value.slice(at) });
    return out;
  });

  const insideIndex = $derived(
    caret === null
      ? -1
      : ranges.findIndex((r) => caret! > r.from && caret! < r.to),
  );
  const activeIndex = $derived(
    hovered !== null && hovered < ranges.length
      ? hovered
      : insideIndex >= 0
        ? insideIndex
        : null,
  );
  // A variable that is defined but not described yet (the inspection is
  // loading, or does not list it) has nothing to show, so no popover.
  const active = $derived.by(() => {
    if (activeIndex === null || activeIndex === dismissed) return null;
    const range = ranges[activeIndex];
    if (!range) return null;
    const shown =
      !variables.defined(range.name) || variables.describe(range.name);
    return shown ? range : null;
  });
  const anchor = $derived.by(() => {
    if (activeIndex === null) return 0;
    const chip = layer?.querySelector<HTMLElement>(
      `[data-variable="${activeIndex}"]`,
    );
    return Math.max(0, (chip?.offsetLeft ?? 0) - scroll);
  });
  let layer = $state<HTMLElement>();

  function sync(): void {
    // A selection change can arrive just after the field is gone.
    if (!input) return;
    scroll = input.scrollLeft;
    const { selectionStart, selectionEnd } = input;
    caret =
      document.activeElement === input && selectionStart === selectionEnd
        ? selectionStart
        : null;
    if (insideIndex !== dismissed && hovered !== dismissed) dismissed = null;
  }

  onMount(() => {
    // The caret moves without any event on the input itself.
    document.addEventListener("selectionchange", sync);
    return () => document.removeEventListener("selectionchange", sync);
  });

  function onkeydown(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      onenter?.();
    } else if (e.key === "Escape" && active) {
      // Only a field with a popover keeps the key from the surrounding view.
      e.stopPropagation();
      dismissed = activeIndex;
    }
  }

  function onchipdown(e: MouseEvent, index: number): void {
    const range = ranges[index];
    if (!range) return;
    e.preventDefault();
    input.focus();
    input.setSelectionRange(range.to, range.to);
    sync();
  }

  function enter(index: number): void {
    hovered = index;
    if (dismissed !== index) dismissed = null;
  }
</script>

<div class="field" class:mono>
  <input
    bind:this={input}
    class="input"
    aria-label={label}
    aria-describedby={active ? popoverId : undefined}
    {placeholder}
    {value}
    autocomplete="off"
    spellcheck="false"
    oninput={(e) => {
      oninput(e.currentTarget.value);
      sync();
    }}
    onscroll={sync}
    onkeyup={sync}
    onclick={sync}
    onfocus={sync}
    onblur={sync}
    onselect={sync}
    {onkeydown}
  />
  <div class="layer" aria-hidden="true">
    <div
      class="text"
      bind:this={layer}
      style:transform="translateX(-{scroll}px)"
    >
      {#each pieces as piece, i (i)}
        {#if piece.variable === undefined}{piece.text}{:else}<!-- svelte-ignore a11y_no_static_element_interactions --><span
            class="variable"
            class:undefined-var={!variables.defined(
              ranges[piece.variable]?.name ?? "",
            )}
            data-variable={piece.variable}
            onmouseenter={() => enter(piece.variable!)}
            onmouseleave={() => (hovered = null)}
            onmousedown={(e) => onchipdown(e, piece.variable!)}
            >{piece.text}</span
          >{/if}
      {/each}
    </div>
  </div>
  {#if active}
    {@const info = variables.describe(active.name)}
    <VariablePopover
      id={popoverId}
      name={active.name}
      left={anchor}
      shown={info?.shown ?? null}
      source={info?.source ?? ""}
      missing={info ? null : variables.undefinedText(active.name)}
    />
  {/if}
</div>

<style>
  .field {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 28px;
    border-radius: var(--r-small);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font-family: var(--font-ui);
    font-size: 13px;
    color: var(--text-1);
  }

  .field.mono {
    font-family: var(--font-mono);
  }

  .field:focus-within {
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  /* The input and the layer share every metric that moves text. */
  .input,
  .text {
    box-sizing: border-box;
    padding: 0 var(--s-2);
    font: inherit;
    letter-spacing: normal;
    line-height: 28px;
    white-space: pre;
  }

  .input {
    width: 100%;
    height: 100%;
    border: 0;
    outline: none;
    background: transparent;
    color: transparent;
    caret-color: var(--text-1);
  }

  .input::placeholder {
    color: var(--text-3);
  }

  .layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border-radius: inherit;
    pointer-events: none;
  }

  .text {
    height: 100%;
    color: var(--text-1);
  }

  .variable {
    pointer-events: auto;
    border-radius: var(--r-key);
    background: var(--bg-3);
    color: var(--text-2);
  }

  .variable.undefined-var {
    color: var(--negative-text);
  }
</style>
