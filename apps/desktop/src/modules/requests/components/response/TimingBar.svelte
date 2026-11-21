<!--
  Where the time of a response went (F1-REQ-08): a bar of five phases and a
  legend. A reused connection shows no DNS, connect or TLS.
-->
<script lang="ts">
  import type { TimingInfo } from "$lib/ipc/bindings/TimingInfo";
  import { formatMs } from "../../lib/format";

  let { timing }: { timing: TimingInfo } = $props();

  const phases = $derived([
    { id: "dns", label: "DNS", short: "DNS", ms: timing.dnsMs },
    { id: "connect", label: "Connect", short: "Connect", ms: timing.connectMs },
    { id: "tls", label: "TLS", short: "TLS", ms: timing.tlsMs },
    {
      id: "waiting",
      label: "Waiting for server",
      short: "Waiting for server",
      ms: timing.waitingMs,
    },
    {
      id: "download",
      label: "Download",
      short: "Download",
      ms: timing.downloadMs,
    },
  ]);
  const share = (ms: number) =>
    timing.totalMs > 0 ? (ms / timing.totalMs) * 100 : 0;

  const description = $derived(
    `Timing, ${Math.round(timing.totalMs)} ms total, ` +
      phases.map((p) => `${p.short} ${Math.round(p.ms)}`).join(", ") +
      (timing.reused ? ", connection reused" : ""),
  );
</script>

<div class="timing">
  <div class="bar" role="img" aria-label={description}>
    {#each phases as phase (phase.id)}
      <span
        class={["segment", phase.id, { filled: phase.ms > 0 }]}
        data-segment={phase.id}
        style:width="{share(phase.ms)}%"
      ></span>
    {/each}
  </div>
  <ul class="legend">
    {#each phases as phase (phase.id)}
      <li>
        <span class={["swatch", phase.id]} aria-hidden="true"></span>
        <span class="name">{phase.label}</span>
        <span class="ms">{formatMs(phase.ms)}</span>
        {#if timing.reused && phase.id === "tls"}
          <span class="reused">reused</span>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .timing {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .bar {
    display: flex;
    gap: 1px;
    height: 6px;
    border-radius: 3px;
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

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-1) var(--s-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }

  .dns {
    background: var(--series-1);
  }

  .connect {
    background: var(--series-2);
  }

  .tls {
    background: var(--series-3);
  }

  .waiting {
    background: var(--accent);
  }

  .download {
    background: var(--series-4);
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
