<!-- The design's buttons: primary, secondary, ghost, danger and danger-solid. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  let {
    variant = "secondary",
    size = "default",
    type = "button",
    children,
    ...rest
  }: HTMLButtonAttributes & {
    variant?: "primary" | "secondary" | "ghost" | "danger" | "danger-solid";
    size?: "default" | "small";
    children?: Snippet;
  } = $props();
</script>

<button {type} class={["btn", variant, { small: size === "small" }]} {...rest}>
  {@render children?.()}
</button>

<style>
  .btn {
    height: 36px;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    white-space: nowrap;
  }

  .small {
    height: 28px;
    padding: 0 var(--s-2);
    gap: 6px;
    border-radius: var(--r-small);
  }

  .primary {
    padding: 0 var(--s-4);
    background: var(--primary-bg);
    color: var(--primary-text);
    font-weight: 500;
    box-shadow: var(--highlight);
  }

  .primary:hover {
    filter: brightness(0.96);
  }

  .secondary {
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .secondary:hover {
    background: var(--bg-hover);
  }

  .ghost {
    color: var(--text-2);
  }

  .ghost:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .danger {
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--negative-text);
  }

  .danger:hover {
    background: var(--bg-hover);
  }

  .danger-solid {
    padding: 0 var(--s-4);
    background: var(--negative-solid);
    color: var(--negative-solid-text);
    font-weight: 500;
  }

  .danger-solid:hover {
    filter: brightness(0.96);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
