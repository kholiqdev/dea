<!-- The environment requests use, in the status bar; red for production. -->
<script lang="ts">
  import { environmentColor, projectApi } from "$app/contributions/project-api";

  const project = projectApi();
  const env = $derived(project.environment);
</script>

{#if env}
  <span class={["env", { "env-prod-status": env.production }]}>
    <span class="dot" style:background={environmentColor(env)}></span>
    <span>{env.name}</span>
  </span>
{/if}

<style>
  .env {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .env-prod-status {
    color: var(--negative-text);
    font-weight: 500;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 3px;
  }
</style>
