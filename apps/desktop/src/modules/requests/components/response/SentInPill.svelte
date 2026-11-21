<!--
  The environment a request was sent in, as its pill: just its name while
  the window is still in it, `Sent in …` once the window moved on.
  Production stands out.
-->
<script lang="ts">
  let {
    name,
    production,
    moved,
  }: {
    name: string;
    production: boolean;
    /** The window is in another environment now. */
    moved: boolean;
  } = $props();

  const text = $derived(moved ? `Sent in ${name}` : name);
</script>

<span class={["env", { "env-prod": production }]} title={text}>{text}</span>

<style>
  /* In a row short of room it gives way, after the status reason. */
  .env {
    flex: 0 1 auto;
    min-width: 0;
    height: 18px;
    padding: 0 6px;
    display: inline-block;
    overflow: hidden;
    border-radius: var(--r-key);
    font: 400 11px/18px var(--font-mono);
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .env-prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }
</style>
