<!-- Records the next chord pressed. The shell's shortcuts stay quiet while
     it has focus (data-capture-keys). Escape cancels. -->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    chordFromEvent,
    chordToString,
    currentPlatform,
    formatChord,
    parseChord,
  } from "$lib/keyboard/chord";

  let {
    label,
    value,
    oncapture,
    oncancel,
  }: {
    label: string;
    value: string | null;
    oncapture: (chord: string) => void;
    oncancel: () => void;
  } = $props();

  const platform = currentPlatform();
  let input: HTMLInputElement;

  onMount(() => input.focus());

  function onkeydown(e: KeyboardEvent): void {
    // Plain Tab still moves focus to the buttons next to the field.
    if (e.key === "Tab" && !e.ctrlKey && !e.metaKey && !e.altKey) return;
    e.preventDefault();
    e.stopPropagation();
    if (
      e.key === "Escape" &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey &&
      !e.shiftKey
    ) {
      oncancel();
      return;
    }
    const chord = chordFromEvent(e, platform);
    // A key without Mod or Alt would take over typing and buttons (Enter,
    // Space, letters); function keys are free to use on their own.
    if (!chord || (!chord.mod && !chord.alt && !/^F\d+$/.test(chord.key)))
      return;
    oncapture(chordToString(chord));
  }
</script>

<input
  bind:this={input}
  class="capture"
  type="text"
  readonly
  data-capture-keys
  aria-label={label}
  placeholder="Press the new shortcut"
  value={value ? formatChord(parseChord(value), platform) : ""}
  {onkeydown}
/>

<style>
  .capture {
    width: 200px;
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-input-strong);
    box-shadow: inset 0 0 0 1px var(--focus);
    font-family: var(--font-mono);
    color: var(--text-1);
    caret-color: transparent;
  }

  .capture:focus-visible {
    outline: none;
  }
</style>
