<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import { toasts } from "./toasts.svelte";

  function chordKeys(chord: string | undefined): string[] {
    if (!chord) return [];
    try {
      return chordParts(parseChord(chord), currentPlatform());
    } catch {
      return [];
    }
  }
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    <div class="toast">
      <span>{toast.text}</span>
      {#if toast.action}
        {@const action = toast.action}
        {@const keys = chordKeys(action.chord)}
        <Button
          size="small"
          onclick={() => {
            action.run();
            toasts.dismiss(toast.id);
          }}
        >
          {action.label}
          {#each keys as key, i (i)}<kbd class="kc">{key}</kbd>{/each}
        </Button>
        {#if toast.seconds !== undefined}
          <!-- Read once with the toast, not every second. -->
          <span class="left" aria-hidden="true">{toast.seconds} s</span>
        {/if}
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

  .kc {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-2);
  }

  .left {
    padding-right: var(--s-2);
    font: 400 11px/1 var(--font-mono);
    color: var(--text-3);
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
