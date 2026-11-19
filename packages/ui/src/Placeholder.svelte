<!-- A module view that is not built yet: its name and "Coming soon". -->
<script lang="ts">
  import type { Component } from "svelte";

  let {
    title,
    icon: Icon,
    text = "Coming soon",
  }: {
    title: string;
    icon?: Component<{ size?: number; strokeWidth?: number }>;
    text?: string;
  } = $props();
</script>

<section class="placeholder">
  {#if Icon}
    <span class="icon" aria-hidden="true"
      ><Icon size={32} strokeWidth={1.5} /></span
    >
  {/if}
  <h1>{title}</h1>
  <p>{text}</p>
</section>

<style>
  .placeholder {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--s-2);
    padding: var(--s-8);
    text-align: center;
  }

  .icon {
    color: var(--text-3);
    margin-bottom: var(--s-2);
  }

  h1 {
    font-size: 1.25em;
    font-weight: 600;
    color: var(--text-1);
  }

  p {
    color: var(--text-2);
  }
</style>
