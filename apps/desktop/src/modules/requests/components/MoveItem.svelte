<!--
  Moves a request or folder to the end of another folder: the keyboard's
  way to do what dragging does.
-->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import type { FolderChoice, TreeRowData } from "../state/tree.svelte";

  let {
    row,
    targets,
    onmove,
    oncancel,
  }: {
    row: TreeRowData;
    targets: FolderChoice[];
    onmove: (folder: string) => void;
    oncancel: () => void;
  } = $props();

  const id = `move-${Math.random().toString(36).slice(2)}`;
  let chosen = $state(0);
  const target = $derived(targets[chosen]);

  function move(): void {
    if (target) onmove(target.path);
  }

  function onkeydown(e: KeyboardEvent): void {
    const last = targets.length - 1;
    const to: Record<string, number> = {
      ArrowDown: Math.min(last, chosen + 1),
      ArrowUp: Math.max(0, chosen - 1),
      Home: 0,
      End: last,
    };
    if (e.key === "Enter") move();
    else if (e.key in to) chosen = Math.max(0, to[e.key] ?? 0);
    else return;
    e.preventDefault();
  }

  function onclick(e: MouseEvent): void {
    const option = (e.target as Element).closest<HTMLElement>(
      "[role='option']",
    );
    if (option?.dataset.index) chosen = Number(option.dataset.index);
  }

  // Keeps the chosen folder in view in a long list.
  $effect(() => {
    document
      .getElementById(`${id}-${chosen}`)
      ?.scrollIntoView?.({ block: "nearest" });
  });
</script>

<Dialog title="Move {row.name}" {oncancel}>
  <div
    class="list"
    role="listbox"
    aria-label="Move to"
    tabindex="0"
    data-autofocus
    aria-activedescendant={target ? `${id}-${chosen}` : undefined}
    {onkeydown}
    {onclick}
    ondblclick={move}
  >
    {#each targets as folder, i (folder.path)}
      <div
        id="{id}-{i}"
        class="option"
        role="option"
        aria-selected={i === chosen}
        data-index={i}
        style:padding-left="calc(var(--s-2) + {folder.depth * 14}px)"
      >
        {folder.name}
      </div>
    {/each}
  </div>
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <Button variant="primary" disabled={!target} onclick={move}>Move</Button>
  {/snippet}
</Dialog>

<style>
  .list {
    max-height: 280px;
    overflow-y: auto;
    padding: var(--s-1);
    border-radius: var(--r-ctl);
    background: var(--bg-1);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .list:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }

  .option {
    height: 28px;
    display: flex;
    align-items: center;
    padding-right: var(--s-2);
    border-radius: var(--r-small);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .option:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .option[aria-selected="true"] {
    background: var(--bg-3);
    box-shadow: var(--highlight);
    color: var(--text-1);
  }
</style>
