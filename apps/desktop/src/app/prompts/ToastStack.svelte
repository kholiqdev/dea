<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import { toasts } from "./toasts.svelte";
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    <div class="toast">
      <span>{toast.text}</span>
      {#if toast.action}
        {@const action = toast.action}
        <Button
          size="small"
          onclick={() => {
            action.run();
            toasts.dismiss(toast.id);
          }}>{action.label}</Button
        >
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: 44px;
    transform: translateX(-50%);
    z-index: 70;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-2);
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    min-height: 40px;
    padding: var(--s-2) var(--s-2) var(--s-2) var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-3);
    color: var(--text-1);
    box-shadow:
      inset 0 0 0 1px var(--line-strong),
      var(--shadow-float);
    white-space: nowrap;
    pointer-events: auto;
  }
</style>
