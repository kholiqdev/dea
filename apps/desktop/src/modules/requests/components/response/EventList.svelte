<!--
  The events of a server-sent stream, each with the time it arrived. It
  follows the stream while the reader is at the bottom, and keeps the last
  10,000.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { SseRow } from "../../state/exchange.svelte";

  let {
    events,
    startedAt,
    dropped = 0,
  }: {
    events: SseRow[];
    /** How many older events the state let go of. */
    dropped?: number;
    /** The `performance.now()` the request started at. */
    startedAt: number;
  } = $props();

  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  /** `14:03:07.042`, in local time. */
  function arrival(atMs: number): string {
    const d = new Date(performance.timeOrigin + startedAt + atMs);
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
  }

  let scroller = $state<HTMLElement>();
  let atBottom = true;

  function onscroll(): void {
    if (!scroller) return;
    atBottom =
      scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 4;
  }

  $effect(() => {
    void events.length;
    void dropped;
    if (!atBottom) return;
    void tick().then(() => {
      if (scroller && atBottom) scroller.scrollTop = scroller.scrollHeight;
    });
  });
</script>

<div class="events">
  <p class="caption">
    {events.length} events
    {#if dropped > 0}
      <span>Showing the last 10,000 events.</span>
    {/if}
  </p>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <ul
    aria-label="Server-sent events"
    tabindex="0"
    bind:this={scroller}
    {onscroll}
  >
    {#each events as item, i (dropped + i)}
      <li>
        <time>{arrival(item.atMs)}</time>
        <span class="name">{item.event ?? "message"}</span>
        <code>{item.data}</code>
      </li>
    {/each}
  </ul>
</div>

<style>
  .events {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .caption {
    flex: none;
    margin: 0;
    padding: var(--s-2) var(--s-4);
    color: var(--text-3);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .caption span {
    margin-left: var(--s-2);
  }

  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow: auto;
  }

  li {
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: var(--s-3);
    padding: var(--s-1) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  time {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-3);
  }

  .name {
    color: var(--accent);
  }

  code {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-1);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
