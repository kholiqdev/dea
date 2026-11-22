<!--
  Where the time of a response went (F1-REQ-08): a small bar of five phases
  that opens or closes their legend, a row of the response header. Whether
  it is open is kept for the session. A reused connection shows no DNS,
  connect or TLS. Muted, as for a previous response, every phase is grey.
-->
<script lang="ts">
  import type { TimingInfo } from "$lib/ipc/bindings/TimingInfo";
  import { sessionView } from "../../state/session-view.svelte";
  import { timingPhases } from "./timing";

  let {
    timing,
    controls,
    muted = false,
  }: {
    timing: TimingInfo;
    /** The id of the legend the bar opens. */
    controls: string;
    /** Drawn without the phases' colours. */
    muted?: boolean;
  } = $props();

  const phases = $derived(timingPhases(timing));
  const share = (ms: number) =>
    timing.totalMs > 0 ? (ms / timing.totalMs) * 100 : 0;

  const description = $derived(
    `Timing, ${Math.round(timing.totalMs)} ms total, ` +
      phases.map((p) => `${p.label} ${Math.round(p.ms)}`).join(", ") +
      (timing.reused ? ", connection reused" : ""),
  );
  const open = $derived(sessionView.timingLegend);
</script>

<button
  type="button"
  class="timing"
  aria-label={description}
  aria-expanded={open}
  aria-controls={controls}
  title={open ? "Hide timing" : "Show timing"}
  onclick={() => (sessionView.timingLegend = !open)}
>
  <span class="bar">
    {#each phases as phase (phase.id)}
      <span
        class={["segment", { filled: phase.ms > 0 }]}
        data-segment={phase.id}
        style:width="{share(phase.ms)}%"
        style:background={muted ? "var(--text-3)" : phase.color}
      ></span>
    {/each}
  </span>
</button>

<style>
  .timing {
    flex: none;
    height: 24px;
    padding: 0 var(--s-1);
    display: inline-flex;
    align-items: center;
    border: 0;
    border-radius: var(--r-small);
    background: none;
  }

  .timing:hover,
  .timing[aria-expanded="true"] {
    background: var(--bg-hover);
  }

  .bar {
    width: 120px;
    height: 4px;
    display: flex;
    gap: 1px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--bg-3);
  }

  .segment {
    flex: none;
    display: block;
  }

  .segment.filled {
    min-width: 2px;
  }
</style>
