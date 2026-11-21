<!--
  The first row of the response header: its status, time, timing bar, size
  and version, the environment it was sent in, and what it can do (copy,
  save, move the panel). It never wraps: the reason gives way first, and a
  narrow row drops the version, then the size, which the timing legend
  under it still shows. While sending it shows the seconds that went by.
-->
<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import PanelBottom from "@lucide/svelte/icons/panel-bottom";
  import PanelRight from "@lucide/svelte/icons/panel-right";
  import { shellApi } from "$app/contributions/shell-api";
  import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
  import { errorMessage } from "$lib/ipc/errors";
  import { copyResponse, saveResponse } from "$lib/ipc/responses";
  import { sessionView } from "../../state/session-view.svelte";
  import {
    formatBytes,
    formatMs,
    formatSeconds,
    statusTone,
    suggestedName,
  } from "../../lib/format";
  import SentInPill from "./SentInPill.svelte";
  import TimingBar from "./TimingBar.svelte";
  import { timingPhases } from "./timing";

  let {
    tabId,
    name,
    response = null,
    elapsed = null,
    environment = null,
    layout,
    stacked,
    ontogglelayout,
  }: {
    tabId: string;
    /** The request's name, which the saved file is named after. */
    name: string;
    response?: ResponseInfo | null;
    /** Milliseconds since the send began; set while sending. */
    elapsed?: number | null;
    /** The environment the request was sent in. */
    environment?: { name: string; production: boolean; moved: boolean } | null;
    layout: "right" | "below";
    /** The panel is below whatever the setting says. */
    stacked: boolean;
    ontogglelayout: () => void;
  } = $props();

  const shell = shellApi();
  const uid = $props.id();

  const copyOff = $derived.by(() => {
    if (!response) return null;
    if (response.large) return "Too large to copy. Save it to a file instead.";
    if (["image", "pdf", "binary"].includes(response.kind))
      return "A binary body cannot be copied as text.";
    return response.kind === "empty" ? "" : null;
  });
  const empty = $derived(response?.kind === "empty");

  async function copy(): Promise<void> {
    try {
      await copyResponse(tabId);
      shell.toast("Response copied");
    } catch (e) {
      shell.toast(errorMessage(e));
    }
  }

  async function save(): Promise<void> {
    if (!response) return;
    try {
      const saved = await saveResponse(
        tabId,
        suggestedName(name, response.kind, response.contentType),
      );
      if (saved !== null) shell.toast(`Saved ${saved}`);
    } catch (e) {
      shell.toast(errorMessage(e));
    }
  }
</script>

<div
  class="toolbar"
  role="toolbar"
  aria-label="Response"
  data-tone={response ? statusTone(response.status) : "muted"}
>
  {#if response}
    <span class="dot" aria-hidden="true"></span>
    <span class="code">{response.status}</span>
    <span class="reason">{response.reason}</span>
    <span class="fact">{formatMs(response.timing.totalMs)}</span>
    <TimingBar timing={response.timing} controls="{uid}-legend" />
    <span class="fact size">{formatBytes(response.size)}</span>
    <span class="fact version">{response.version}</span>
    {#if environment}
      <SentInPill {...environment} />
    {/if}
    <span class="actions">
      <button
        type="button"
        class="icon"
        aria-label="Copy response"
        title={copyOff || "Copy response"}
        disabled={copyOff !== null}
        onclick={() => void copy()}
      >
        <Copy size={16} strokeWidth={1.5} />
      </button>
      <button
        type="button"
        class="icon"
        aria-label="Save response to file"
        title="Save response to file"
        disabled={empty}
        onclick={() => void save()}
      >
        <Download size={16} strokeWidth={1.5} />
      </button>
      {#if !stacked}
        {@const label =
          layout === "right"
            ? "Show response below"
            : "Show response on the right"}
        <button
          type="button"
          class="icon"
          aria-label={label}
          title={label}
          onclick={ontogglelayout}
        >
          {#if layout === "right"}
            <PanelBottom size={16} strokeWidth={1.5} />
          {:else}
            <PanelRight size={16} strokeWidth={1.5} />
          {/if}
        </button>
      {/if}
    </span>
  {:else}
    <span class="sending">Sending</span>
    {#if elapsed !== null}<span class="fact">{formatSeconds(elapsed)}</span
      >{/if}
    {#if environment}
      <SentInPill {...environment} />
    {/if}
  {/if}
</div>
{#if response && sessionView.timingLegend}
  {@const timing = response.timing}
  <div class="legend" id="{uid}-legend">
    <ul class="phases" aria-label="Timing">
      {#each timingPhases(timing) as phase (phase.id)}
        <li>
          <span class="swatch" style:background={phase.color} aria-hidden="true"
          ></span>
          <span>{phase.label}</span>
          <span class="ms">{formatMs(phase.ms)}</span>
          {#if timing.reused && phase.id === "tls"}
            <span class="reused">reused</span>
          {/if}
        </li>
      {/each}
    </ul>
    <span class="fact size">{formatBytes(response.size)}</span>
    <span class="fact version">{response.version}</span>
  </div>
{/if}

<style>
  /* One row whatever its width. A narrow row leaves out the version, then
     the size; short of room, the reason gives way first, then the
     environment. */
  .toolbar {
    container-type: inline-size;
    height: 40px;
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 0 var(--s-4);
    overflow: hidden;
    white-space: nowrap;
    box-shadow: inset 0 -1px 0 var(--line);
    --tone: var(--text-3);
    --tone-text: var(--text-2);
  }

  .toolbar[data-tone="positive"] {
    --tone: var(--positive);
    --tone-text: var(--positive-text);
  }

  .toolbar[data-tone="accent"] {
    --tone: var(--accent);
    --tone-text: var(--warning-text);
  }

  .toolbar[data-tone="warning"] {
    --tone: var(--warning);
    --tone-text: var(--warning-text);
  }

  .toolbar[data-tone="negative"] {
    --tone: var(--negative);
    --tone-text: var(--negative-text);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 4px;
    background: var(--tone);
  }

  .code {
    flex: none;
    font: 600 15px/20px var(--font-mono);
    color: var(--tone-text);
  }

  /* 6 px from its code; shrinks long before anything else does. */
  .reason {
    flex: 0 1000 auto;
    min-width: 0;
    margin-left: calc(6px - var(--s-3));
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 13px;
    font-weight: 400;
    color: var(--text-2);
  }

  .sending {
    flex: none;
    font-weight: 500;
    color: var(--text-1);
  }

  .fact {
    flex: none;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  @container (width < 560px) {
    .toolbar .version {
      display: none;
    }
  }

  @container (width < 480px) {
    .toolbar .size {
      display: none;
    }
  }

  .actions {
    flex: none;
    display: flex;
    margin-left: auto;
  }

  .icon {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .icon:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .icon:disabled {
    color: var(--text-4);
  }

  /* The timing legend, under the row: its phases, then the size and the
     version when the row above leaves them out. Both rows are as wide as
     the pane. Short of room, the phases scroll; the size and version stay
     in view. */
  .legend {
    container-type: inline-size;
    height: 28px;
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-4);
    padding: 0 var(--s-4);
    white-space: nowrap;
    box-shadow: inset 0 -1px 0 var(--line);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .phases {
    flex: 0 1 auto;
    min-width: 0;
    display: flex;
    gap: var(--s-4);
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  @container (width >= 560px) {
    .legend .version {
      display: none;
    }
  }

  @container (width >= 480px) {
    .legend .size {
      display: none;
    }
  }

  /* Where phases that do not fit are cut off, before the version. */
  @container (width < 560px) {
    .phases {
      padding-right: var(--s-4);
      box-shadow: inset -1px 0 0 var(--line);
    }
  }

  .phases li {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }

  .ms {
    font-family: var(--font-mono);
    color: var(--text-2);
  }

  .reused {
    padding: 0 6px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    color: var(--text-2);
  }
</style>
