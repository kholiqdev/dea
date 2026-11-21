<!--
  What came back of a tab's send (F1-REQ-08): the toolbar and timing of a
  response, the progress of a request on its way, or why it ended without
  one. The body itself goes in the `body` slot below.
-->
<script lang="ts">
  import { projectApi } from "$app/contributions/project-api";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import { formatSeconds } from "../../lib/format";
  import type { Exchange } from "../../state/exchange.svelte";
  import FailureView from "./FailureView.svelte";
  import ResponseToolbar from "./ResponseToolbar.svelte";
  import SentInPill from "./SentInPill.svelte";
  import SendingView from "./SendingView.svelte";
  import TimingBar from "./TimingBar.svelte";

  let {
    tabId,
    name,
    exchange,
    layout,
    stacked,
    ontogglelayout,
  }: {
    tabId: string;
    name: string;
    exchange: Exchange;
    layout: "right" | "below";
    stacked: boolean;
    ontogglelayout: () => void;
  } = $props();

  const project = projectApi();
  const sendKeys = chordParts(parseChord("Mod+Enter"), currentPlatform());
  const current = $derived(exchange.state);

  // Seconds go by on the clock `startedAt` was taken from.
  let now = $state(performance.now());
  $effect(() => {
    if (current.phase !== "sending") return;
    now = performance.now();
    const timer = setInterval(() => (now = performance.now()), 100);
    return () => clearInterval(timer);
  });
  const elapsed = $derived(
    current.phase === "sending" ? Math.max(0, now - current.startedAt) : 0,
  );

  // The environment a response was sent in, when the window moved on.
  const sentIn = $derived.by(() => {
    if (current.phase !== "done" && current.phase !== "failed") return null;
    const sent = current.environment;
    if (sent === null || sent === project.environment?.name) return null;
    const production =
      project.info?.environments.find((e) => e.name === sent)?.production ??
      false;
    return { name: sent, production };
  });
</script>

<section
  class="pane"
  aria-label="Response"
  aria-busy={current.phase === "sending" ? "true" : undefined}
>
  {#if current.phase === "idle"}
    <p class="idle">
      Send the request to see the response here.
      <span class="keys" aria-hidden="true">
        {#each sendKeys as key, i (i)}<kbd>{key}</kbd>{/each}
      </span>
    </p>
  {:else if current.phase === "sending"}
    <ResponseToolbar
      {tabId}
      {name}
      {elapsed}
      {layout}
      {stacked}
      {ontogglelayout}
    />
    <SendingView sending={current} {elapsed} />
  {:else if current.phase === "done"}
    <ResponseToolbar
      {tabId}
      {name}
      response={current.response}
      {sentIn}
      {layout}
      {stacked}
      {ontogglelayout}
    />
    <div class="timing"><TimingBar timing={current.response.timing} /></div>
    <!-- Task 14 puts the Body, Headers and Cookies views here. -->
    <div class="body" data-slot="response-body"></div>
  {:else if current.phase === "failed"}
    {#if sentIn}<p class="sent-in"><SentInPill {...sentIn} /></p>{/if}
    <FailureView failure={current.failure} />
  {:else}
    <div class="cancelled">
      <p>Cancelled after {formatSeconds(current.afterMs)}.</p>
      {#if current.head}
        <p>
          {current.head.status}
          {current.head.reason}. The body was not received in full.
        </p>
      {/if}
    </div>
  {/if}
</section>

<style>
  .pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }

  .idle {
    margin: auto;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    color: var(--text-3);
  }

  .keys {
    display: inline-flex;
    gap: 4px;
  }

  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-2);
  }

  .timing {
    flex: none;
    padding: var(--s-3) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .body {
    flex: 1;
    min-height: 0;
  }

  .cancelled {
    padding: var(--s-4);
    color: var(--text-2);
  }

  .cancelled p,
  .sent-in {
    margin: 0;
  }

  .sent-in {
    padding: var(--s-3) var(--s-4) 0;
  }
</style>
