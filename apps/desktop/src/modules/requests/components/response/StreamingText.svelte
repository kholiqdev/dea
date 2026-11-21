<!--
  The text of a body as it arrives, plain until the response is complete.
  Past the limit Dea stops forwarding text; only the count goes on.
-->
<script lang="ts">
  import { formatBytes } from "../../lib/format";

  let {
    text,
    textBytes,
    received,
  }: {
    text: string;
    /** The bytes of `text`, which count against `received`. */
    textBytes: number;
    received: number;
  } = $props();
</script>

<div class="streaming">
  <p class="caption">
    Receiving… {formatBytes(received)}{received > textBytes
      ? " Shown when it is complete."
      : ""}
  </p>
  <pre aria-live="off">{text}</pre>
</div>

<style>
  .streaming {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .caption {
    flex: none;
    margin: 0;
    color: var(--text-3);
  }

  pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 18px;
    color: var(--text-1);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
