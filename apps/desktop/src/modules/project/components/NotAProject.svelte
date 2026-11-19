<script lang="ts">
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Button from "@dea/ui/Button.svelte";

  let {
    path,
    oncreate,
    onchoose,
  }: { path: string; oncreate: () => void; onchoose: () => void } = $props();
</script>

<div class="card" role="alert">
  <span class="why">
    <TriangleAlert size={16} strokeWidth={1.5} />
    <span
      ><span class="mono">{path}</span> has no
      <span class="mono">dea.yaml</span>, so it is not a Dea project.</span
    >
  </span>
  <span class="buttons">
    <Button size="small" onclick={oncreate}>Create a project there</Button>
    <Button size="small" variant="ghost" onclick={onchoose}
      >Choose another folder</Button
    >
  </span>
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-2);
  }

  .why {
    display: flex;
    gap: var(--s-2);
    align-items: flex-start;
    color: var(--warning-text);
  }

  .why > span {
    color: var(--text-2);
  }

  .mono {
    font-family: var(--font-mono);
  }

  .buttons {
    display: flex;
    gap: var(--s-2);
  }
</style>
