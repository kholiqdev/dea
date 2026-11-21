<!--
  A request on its way: the phases of the connection as they end, what it
  waits for now, and a skeleton where the body will be.
-->
<script lang="ts">
  import { formatBytes, formatMs, formatSeconds } from "../../lib/format";
  import type { ExchangeState } from "../../state/exchange.svelte";

  let {
    sending,
    elapsed,
  }: {
    sending: Extract<ExchangeState, { phase: "sending" }>;
    /** Milliseconds since the send began. */
    elapsed: number;
  } = $props();

  const LABELS = { dns: "DNS lookup", connect: "Connect", tls: "TLS" } as const;
  const downloading = $derived(sending.head !== null);
</script>

<div class="sending">
  <div
    class="progress"
    role="progressbar"
    aria-label={downloading ? "Downloading" : "Waiting for the server"}
  >
    <span class="run"></span>
  </div>
  <ul class="steps">
    {#each sending.steps as step, i (i)}
      <li>
        {#if step.kind === "reused"}
          Reused connection
        {:else}
          {LABELS[step.kind]} {formatMs(step.ms)}
        {/if}
      </li>
    {/each}
    {#if downloading}
      <li>Downloading {formatBytes(sending.received)}</li>
    {:else}
      <li class="now">
        Waiting for {sending.host ?? "the server"}
        {formatSeconds(elapsed)}
      </li>
    {/if}
  </ul>
  <div class="skeleton" aria-hidden="true">
    {#each [92, 64, 78, 40, 70, 52] as width, i (i)}
      <span style:width="{width}%"></span>
    {/each}
  </div>
</div>

<style>
  .sending {
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    padding: var(--s-4);
  }

  .progress {
    height: 2px;
    border-radius: 1px;
    background: var(--bg-3);
    overflow: hidden;
  }

  .run {
    display: block;
    width: 30%;
    height: 100%;
    background: var(--accent);
    animation: slide 1.2s var(--ease) infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .run {
      width: 100%;
      animation: none;
    }
  }

  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .now {
    color: var(--text-1);
  }

  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .skeleton span {
    height: 10px;
    border-radius: var(--r-key);
    background: var(--bg-3);
  }
</style>
