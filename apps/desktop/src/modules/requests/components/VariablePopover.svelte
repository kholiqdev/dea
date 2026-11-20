<!-- A variable's final value and where it comes from, or why it has none. -->
<script lang="ts">
  let {
    id,
    name,
    left,
    shown,
    source,
    missing,
  }: {
    id: string;
    name: string;
    left: number;
    /** The value, or null when the variable is not defined. */
    shown: string | null;
    source: string;
    /** The explanation shown instead of a value when not defined. */
    missing: string | null;
  } = $props();
</script>

<div {id} role="tooltip" class="popover" style:left="{left}px">
  <code class="name">{`{{${name}}}`}</code>
  {#if missing !== null}
    <p class="missing">{missing}</p>
  {:else}
    <p class="value">{shown}</p>
    <p class="from">From</p>
    <p class="source">{source}</p>
  {/if}
</div>

<style>
  .popover {
    position: absolute;
    top: calc(100% + var(--s-1));
    z-index: 10;
    min-width: 200px;
    max-width: 360px;
    padding: var(--s-2) var(--s-3);
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow:
      inset 0 0 0 1px var(--line-strong),
      var(--shadow-float);
    color: var(--text-1);
    font-family: var(--font-ui);
    font-size: 12px;
    pointer-events: none;
  }

  p {
    margin: 0;
  }

  .name {
    font-family: var(--font-mono);
    color: var(--text-2);
  }

  .value {
    margin-top: var(--s-1);
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
  }

  .from {
    margin-top: var(--s-2);
    color: var(--text-3);
  }

  .source,
  .missing {
    margin-top: var(--s-1);
    color: var(--text-2);
  }

  .missing {
    color: var(--negative-text);
  }
</style>
