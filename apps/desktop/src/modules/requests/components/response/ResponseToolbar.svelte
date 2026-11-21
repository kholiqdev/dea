<!--
  The line above a response: its status, time, size and version, what it
  can do (copy, save, move the panel), and the environment it was sent in
  when that is no longer the window's. While sending it only shows the
  seconds that went by.
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
  import {
    formatBytes,
    formatMs,
    formatSeconds,
    statusTone,
    suggestedName,
  } from "../../lib/format";
  import SentInPill from "./SentInPill.svelte";

  let {
    tabId,
    name,
    response = null,
    elapsed = null,
    sentIn = null,
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
    sentIn?: { name: string; production: boolean } | null;
    layout: "right" | "below";
    /** The panel is below whatever the setting says. */
    stacked: boolean;
    ontogglelayout: () => void;
  } = $props();

  const shell = shellApi();

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
    <span class="status">{response.status} {response.reason}</span>
    <span class="fact">{formatMs(response.timing.totalMs)}</span>
    <span class="fact">{formatBytes(response.size)}</span>
    <span class="fact">{response.version}</span>
    {#if sentIn}
      <SentInPill {...sentIn} />
    {/if}
    <span class="spacer"></span>
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
  {:else}
    <span class="status">Sending</span>
    {#if elapsed !== null}<span class="fact">{formatSeconds(elapsed)}</span
      >{/if}
  {/if}
</div>

<style>
  .toolbar {
    height: 40px;
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 0 var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
    --tone: var(--text-3);
  }

  .toolbar[data-tone="positive"] {
    --tone: var(--positive);
  }

  .toolbar[data-tone="accent"] {
    --tone: var(--accent);
  }

  .toolbar[data-tone="warning"] {
    --tone: var(--warning);
  }

  .toolbar[data-tone="negative"] {
    --tone: var(--negative);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 4px;
    background: var(--tone);
  }

  .status {
    font-weight: 500;
    color: var(--text-1);
  }

  .fact {
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .spacer {
    flex: 1;
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
</style>
