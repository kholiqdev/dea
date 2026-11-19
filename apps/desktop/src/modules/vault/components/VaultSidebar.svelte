<!-- The environments of the project, with the values this device has. -->
<script lang="ts">
  import { environmentColor, projectApi } from "$app/contributions/project-api";
  import { secrets } from "../state/secrets.svelte";

  const project = projectApi();
  const info = $derived(project.info);
  const current = $derived(info ? secrets.current(info) : null);
</script>

<nav class="list" aria-label="Environments">
  <div class="head">
    <h2>Vault</h2>
    {#if info}<span class="project">{info.name}</span>{/if}
  </div>
  {#each info?.environments ?? [] as env (env.name)}
    <button
      type="button"
      class="srow"
      aria-current={env.name === current ? "true" : undefined}
      onclick={() => (secrets.chosen = env.name)}
    >
      <span class="dot" style:background={environmentColor(env)}></span>
      <span class="label">{env.name}</span>
      <span class="count">{secrets.count(env.name)}</span>
    </button>
  {/each}
</nav>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .head {
    display: flex;
    flex-direction: column;
    padding: var(--s-2);
  }

  h2 {
    font-size: 13px;
    font-weight: 500;
  }

  .project,
  .count {
    font-size: 12px;
    color: var(--text-3);
  }

  .count {
    font-family: var(--font-mono);
  }

  .srow {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--text-2);
    text-align: left;
  }

  .srow:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .srow[aria-current="true"] {
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
  }

  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 4px;
  }
</style>
