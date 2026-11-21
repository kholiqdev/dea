<!-- The environment a response was sent in, as its pill; production stands out. -->
<script lang="ts">
  let { name, production }: { name: string; production: boolean } = $props();
</script>

<span class={["env", { "env-prod": production }]}>Sent in {name}</span>

<style>
  .env {
    height: 18px;
    padding: 0 6px;
    display: inline-flex;
    align-items: center;
    border-radius: var(--r-key);
    font: 400 11px/1 var(--font-mono);
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .env-prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }
</style>
