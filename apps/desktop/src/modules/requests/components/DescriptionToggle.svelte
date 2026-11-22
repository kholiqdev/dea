<!--
  The button in the last cell of a table's header row. It shows or hides the
  Description column for every request table. While the column is hidden, a
  dot says that some row has a description.
-->
<script lang="ts">
  import AlignLeft from "@lucide/svelte/icons/align-left";
  import { sessionView } from "../state/session-view.svelte";

  let { hasDescriptions }: { hasDescriptions: boolean } = $props();

  const shown = $derived(sessionView.descriptions);
  const label = $derived(shown ? "Hide descriptions" : "Show descriptions");
</script>

<button
  type="button"
  class="toggle"
  aria-label={label}
  title={label}
  aria-pressed={shown}
  data-has-descriptions={!shown && hasDescriptions ? "" : undefined}
  onclick={() => (sessionView.descriptions = !shown)}
>
  <AlignLeft size={14} strokeWidth={1.5} />
</button>

<style>
  .toggle {
    position: relative;
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-3);
  }

  .toggle:hover,
  .toggle[aria-pressed="true"] {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .toggle[data-has-descriptions]::after {
    content: "";
    position: absolute;
    top: 3px;
    right: 3px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
