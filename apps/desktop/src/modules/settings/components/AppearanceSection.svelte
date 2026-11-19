<script lang="ts">
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import { errorMessage } from "$lib/ipc/errors";
  import type { Appearance } from "$lib/ipc/bindings/Appearance";
  import { settings } from "$lib/settings/settings.svelte";
  import AppearancePreview from "./AppearancePreview.svelte";

  const MIN = 11;
  const MAX = 20;
  const THEMES = [
    ["dark", "Dark"],
    ["light", "Light"],
    ["system", "Match system"],
  ] as const;
  // A short list plus any installed font typed by name; Dea does not scan
  // the system's fonts.
  const UI_FONTS = ["IBM Plex Sans", "System UI"];
  const MONO_FONTS = ["IBM Plex Mono", "System monospace"];

  const a = $derived(settings.appearance);
  let error = $state<string | null>(null);
  let typingUi = $state(false);
  let typingMono = $state(false);
  let sizeInput = $state<HTMLInputElement>();

  async function update(patch: Partial<Appearance>): Promise<void> {
    try {
      await settings.update({ appearance: patch });
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function setSize(value: number): void {
    const size = Math.min(MAX, Math.max(MIN, Math.round(value)));
    if (sizeInput) sizeInput.value = String(size);
    if (size !== a.fontSize) void update({ fontSize: size });
  }

  function commitSize(): void {
    const value = Number(sizeInput?.value);
    if (Number.isFinite(value) && sizeInput?.value !== "") setSize(value);
    else if (sizeInput) sizeInput.value = String(a.fontSize);
  }

  function pick(which: "uiFont" | "monoFont", value: string): void {
    const typing = value === "other";
    if (which === "uiFont") typingUi = typing;
    else typingMono = typing;
    if (!typing) void update({ [which]: value });
  }

  function commitName(
    which: "uiFont" | "monoFont",
    input: HTMLInputElement,
  ): void {
    const name = input.value.trim();
    if (name) void update({ [which]: name });
  }
</script>

{#snippet fontRow(
  label: string,
  which: "uiFont" | "monoFont",
  list: string[],
  typing: boolean,
  note: string | null,
)}
  {@const current = a[which]}
  {@const other = typing || !list.includes(current)}
  <div class="frow">
    <span class="lbl">{label}</span>
    <div class="field">
      <select
        class="input"
        aria-label={label}
        value={other ? "other" : current}
        onchange={(e) => pick(which, e.currentTarget.value)}
      >
        {#each list as font (font)}
          <option value={font}>{font}</option>
        {/each}
        <option value="other">Other font…</option>
      </select>
      {#if other}
        <input
          class="input"
          type="text"
          aria-label="{label} name"
          placeholder="Name of an installed font"
          value={list.includes(current) ? "" : current}
          onkeydown={(e) =>
            e.key === "Enter" && commitName(which, e.currentTarget)}
          onblur={(e) => commitName(which, e.currentTarget)}
        />
      {/if}
      {#if note}<span class="meta">{note}</span>{/if}
    </div>
  </div>
{/snippet}

<div class="page">
  <div class="page-head"><h1>Appearance</h1></div>

  <section class="group">
    <h3 class="field-label" id="theme-label">Theme</h3>
    <div class="seg" role="group" aria-labelledby="theme-label">
      {#each THEMES as [value, label] (value)}
        <button
          type="button"
          aria-pressed={a.theme === value}
          onclick={() => update({ theme: value })}>{label}</button
        >
      {/each}
    </div>
  </section>

  <section class="group">
    <h3 class="field-label">Fonts</h3>
    <div class="card fonts">
      {@render fontRow("Interface font", "uiFont", UI_FONTS, typingUi, null)}
      {@render fontRow(
        "Monospace font",
        "monoFont",
        MONO_FONTS,
        typingMono,
        "Used in editors, terminals, tables and logs",
      )}
      <div class="frow">
        <span class="lbl" id="size-label">Size</span>
        <div class="size">
          <button
            type="button"
            class="iconbtn"
            aria-label="Smaller"
            title="Smaller"
            disabled={a.fontSize <= MIN}
            onclick={() => setSize(a.fontSize - 1)}
          >
            <Minus size={16} strokeWidth={1.5} />
          </button>
          <input
            bind:this={sizeInput}
            class="input mono"
            type="number"
            min={MIN}
            max={MAX}
            aria-label="Size in px"
            value={a.fontSize}
            onkeydown={(e) => e.key === "Enter" && commitSize()}
            onchange={commitSize}
          />
          <button
            type="button"
            class="iconbtn"
            aria-label="Larger"
            title="Larger"
            disabled={a.fontSize >= MAX}
            onclick={() => setSize(a.fontSize + 1)}
          >
            <Plus size={16} strokeWidth={1.5} />
          </button>
          <span class="meta">px, from {MIN} to {MAX}</span>
        </div>
      </div>
    </div>
  </section>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <section class="group">
    <h3 class="field-label">Preview</h3>
    <AppearancePreview />
  </section>
</div>

<style>
  .page {
    max-width: 760px;
    padding: var(--s-8);
    display: flex;
    flex-direction: column;
    gap: var(--s-8);
  }

  .page-head h1 {
    font-size: 24px;
    line-height: 32px;
    font-weight: 500;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .field-label {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .seg {
    align-self: flex-start;
    display: flex;
    padding: 2px;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .seg button {
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .seg button[aria-pressed="true"] {
    background: var(--bg-3);
    color: var(--text-1);
  }

  .card {
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .fonts {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    padding: var(--s-4);
  }

  .frow {
    display: grid;
    grid-template-columns: 140px minmax(0, 1fr);
    align-items: start;
    column-gap: var(--s-4);
  }

  .lbl {
    line-height: 36px;
    color: var(--text-2);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    width: 280px;
  }

  .input {
    height: 36px;
    width: 100%;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-input-strong);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .input:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .mono {
    font-family: var(--font-mono);
  }

  .size {
    display: flex;
    align-items: center;
    gap: var(--s-1);
  }

  .size .input {
    width: 56px;
    padding: 0 var(--s-2);
    text-align: center;
  }

  .iconbtn {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-3);
  }

  .iconbtn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .iconbtn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .size .meta {
    margin-left: var(--s-2);
  }

  .error {
    color: var(--negative-text);
  }
</style>
