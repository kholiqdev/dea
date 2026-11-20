<!--
  Connection options of a request or a folder. A row this level does not set
  says `Inherited` and shows the value in force; a row it sets has `Reset`.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { OptionsInfo } from "$lib/ipc/bindings/OptionsInfo";
  import type { SendOptions } from "$lib/ipc/bindings/SendOptions";
  import { DEFAULT_OPTIONS } from "../state/folders.svelte";

  let {
    options,
    effective,
    scope,
    onchange,
    disabled = false,
  }: {
    options: SendOptions;
    /** What is in force without this level's own settings; null if unknown. */
    effective: OptionsInfo | null;
    scope: "request" | "folder";
    onchange: (options: SendOptions) => void;
    disabled?: boolean;
  } = $props();

  const uid = $props.id();
  const base = $derived(effective ?? DEFAULT_OPTIONS);
  const shown: OptionsInfo = $derived({
    timeoutMs: options.timeoutMs ?? base.timeoutMs,
    followRedirects: options.followRedirects ?? base.followRedirects,
    maxRedirects: options.maxRedirects ?? base.maxRedirects,
    verifyTls: options.verifyTls ?? base.verifyTls,
    useSystemProxy: options.useSystemProxy ?? base.useSystemProxy,
    cookies: options.cookies ?? base.cookies,
  });

  const isSet = (...keys: (keyof SendOptions)[]) =>
    keys.some((k) => options[k] !== undefined);

  /** The options with `patch` applied; a field set to undefined is dropped. */
  function change(patch: SendOptions): void {
    const next: SendOptions = { ...options, ...patch };
    for (const key of Object.keys(next) as (keyof SendOptions)[])
      if (next[key] === undefined) delete next[key];
    onchange(next);
  }

  function reset(...keys: (keyof SendOptions)[]): void {
    change(Object.fromEntries(keys.map((k) => [k, undefined])));
  }

  type NumberKey = "timeoutMs" | "maxRedirects";

  /** A number typed so far: kept while typing, so the field is not rewritten. */
  function typed(key: NumberKey, text: string): void {
    const n = Number.parseInt(text, 10);
    if (Number.isInteger(n) && n >= 0) change({ [key]: n });
  }

  /** A field left empty or invalid goes back to inheriting. */
  async function committed(
    key: NumberKey,
    input: HTMLInputElement,
  ): Promise<void> {
    const n = Number.parseInt(input.value, 10);
    change({ [key]: Number.isInteger(n) && n >= 0 ? n : undefined });
    await tick();
    input.value = String(shown[key]);
  }
</script>

{#snippet status(set: boolean, keys: (keyof SendOptions)[], name: string)}
  {#if set}
    <button
      type="button"
      class="reset"
      aria-label="Reset {name}"
      {disabled}
      onclick={() => reset(...keys)}>Reset</button
    >
  {:else}
    <span class="inherited">Inherited</span>
  {/if}
{/snippet}

<div class="card">
  <div class="row" role="group" aria-labelledby="{uid}-timeout">
    <span class="label" id="{uid}-timeout">Timeout</span>
    <div class="control">
      <input
        type="number"
        class="number"
        min="0"
        aria-label="Timeout"
        value={shown.timeoutMs}
        {disabled}
        oninput={(e) => typed("timeoutMs", e.currentTarget.value)}
        onchange={(e) => void committed("timeoutMs", e.currentTarget)}
      />
      <span class="unit">ms</span>
      {#if shown.timeoutMs === 0}<span class="help">No limit</span>{/if}
    </div>
    {@render status(isSet("timeoutMs"), ["timeoutMs"], "Timeout")}
  </div>

  <div class="row" role="group" aria-labelledby="{uid}-redirects">
    <span class="label" id="{uid}-redirects">Redirects</span>
    <div class="control">
      <label class="switch">
        <input
          type="checkbox"
          role="switch"
          checked={shown.followRedirects}
          {disabled}
          onchange={(e) => change({ followRedirects: e.currentTarget.checked })}
        />
        Follow redirects
      </label>
      <label class="inline">
        Follow up to
        <input
          type="number"
          class="number small"
          min="0"
          value={shown.maxRedirects}
          disabled={disabled || !shown.followRedirects}
          oninput={(e) => typed("maxRedirects", e.currentTarget.value)}
          onchange={(e) => void committed("maxRedirects", e.currentTarget)}
        />
      </label>
    </div>
    {@render status(
      isSet("followRedirects", "maxRedirects"),
      ["followRedirects", "maxRedirects"],
      "Redirects",
    )}
  </div>

  <div class="row" role="group" aria-labelledby="{uid}-tls">
    <span class="label" id="{uid}-tls">Verify TLS</span>
    <div class="control">
      <label class="switch">
        <input
          type="checkbox"
          role="switch"
          checked={shown.verifyTls}
          {disabled}
          onchange={(e) => change({ verifyTls: e.currentTarget.checked })}
        />
        Verify TLS
      </label>
      {#if !shown.verifyTls}
        <span class="help warning"
          >Off. Anyone on the network path can read or change these requests</span
        >
      {/if}
    </div>
    {@render status(isSet("verifyTls"), ["verifyTls"], "Verify TLS")}
  </div>

  <div class="row" role="group" aria-labelledby="{uid}-proxy">
    <span class="label" id="{uid}-proxy">Proxy</span>
    <div class="control">
      <div class="segmented" role="radiogroup" aria-label="Proxy">
        {#each [{ label: "None", system: false }, { label: "System", system: true }] as choice (choice.label)}
          <button
            type="button"
            role="radio"
            aria-checked={shown.useSystemProxy === choice.system}
            {disabled}
            onclick={() => change({ useSystemProxy: choice.system })}
          >
            {choice.label}
          </button>
        {/each}
      </div>
    </div>
    {@render status(isSet("useSystemProxy"), ["useSystemProxy"], "Proxy")}
  </div>

  {#if scope === "request"}
    <div class="row" role="group" aria-labelledby="{uid}-cookies">
      <span class="label" id="{uid}-cookies">Cookies</span>
      <div class="control">
        <label class="switch">
          <input
            type="checkbox"
            role="switch"
            checked={shown.cookies}
            {disabled}
            onchange={(e) => change({ cookies: e.currentTarget.checked })}
          />
          Use the cookie jar
        </label>
      </div>
      {@render status(isSet("cookies"), ["cookies"], "Cookies")}
    </div>
  {/if}
</div>

<style>
  .card {
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .row {
    min-height: 48px;
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr) 64px;
    align-items: center;
    column-gap: var(--s-3);
    padding: var(--s-1) var(--s-3);
  }

  .row + .row {
    border-top: 1px solid var(--line);
  }

  .label {
    color: var(--text-2);
  }

  .control {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s-3);
    min-width: 0;
  }

  .switch,
  .inline {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    color: var(--text-1);
  }

  .number {
    width: 96px;
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-1);
    font-family: var(--font-mono);
  }

  .number.small {
    width: 64px;
  }

  .number:disabled {
    opacity: 0.5;
  }

  .unit,
  .help,
  .inherited {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .warning {
    color: var(--warning-text);
  }

  .inherited {
    justify-self: end;
  }

  .reset {
    justify-self: end;
    height: 24px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
    font-size: 12px;
  }

  .reset:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .segmented {
    display: inline-flex;
    padding: 2px;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .segmented button {
    height: 24px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .segmented button[aria-checked="true"] {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .segmented button:disabled {
    opacity: 0.5;
  }
</style>
