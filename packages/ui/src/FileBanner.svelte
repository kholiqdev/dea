<!--
  A bar over an editor saying what happened to its file outside Dea, with
  what can be done about it. `tone` colours the icon: warning for a change
  to decide on, negative for a file that is broken, neutral for a fact.
-->
<script module lang="ts">
  export interface BannerAction {
    label: string;
    onclick: () => void;
    /** Ghost is the quieter choice, such as discarding. */
    variant?: "secondary" | "ghost";
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  let {
    tone,
    icon,
    children,
    actions = [],
  }: {
    tone: "warning" | "negative" | "neutral";
    icon?: Snippet;
    /** The message; a `<code>` in it shows a path. */
    children: Snippet;
    actions?: BannerAction[];
  } = $props();
</script>

<div class={["banner", tone]} role="status">
  {#if icon}<span class="icon" aria-hidden="true">{@render icon()}</span>{/if}
  <p class="message">{@render children()}</p>
  {#if actions.length > 0}
    <div class="actions">
      {#each actions as action (action.label)}
        <Button
          size="small"
          variant={action.variant ?? "secondary"}
          onclick={action.onclick}
        >
          {action.label}
        </Button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    min-height: 44px;
    padding: var(--s-2) var(--s-4);
    background: var(--bg-2);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .icon {
    display: inline-flex;
    flex: none;
  }

  .warning .icon {
    color: var(--warning-text);
  }

  .negative .icon {
    color: var(--negative-text);
  }

  .neutral .icon {
    color: var(--text-3);
  }

  .message {
    flex: 1;
    min-width: 0;
    line-height: 20px;
    color: var(--text-1);
  }

  .message :global(code) {
    font-family: var(--font-mono);
  }

  .actions {
    display: flex;
    flex: none;
    gap: var(--s-2);
  }
</style>
