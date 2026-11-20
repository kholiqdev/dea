<!--
  What can be done to a row of the collection tree, or to the tree itself
  (`row` null): a menu under its More actions button, or at `at` (window
  pixels) when opened with a right click. The menu is fixed to the window
  so the sidebar's scrolling does not clip it.
-->
<script lang="ts">
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import Menu from "@dea/ui/Menu.svelte";
  import { shellApi } from "$app/contributions/shell-api";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import { ROOT, treeState, type TreeRowData } from "../state/tree.svelte";

  let {
    row,
    open = $bindable(false),
    at = null,
    hidden = false,
    onmove,
    ondelete,
    onclose,
  }: {
    row: TreeRowData | null;
    open?: boolean;
    at?: { x: number; y: number } | null;
    /** Keeps the button out of sight, for the tree's own menu. */
    hidden?: boolean;
    onmove: (row: TreeRowData) => void;
    ondelete: (row: TreeRowData) => void;
    /** The menu closed and gave the focus back to its button. */
    onclose?: () => void;
  } = $props();

  // A menu item, as Menu takes it.
  type Entry = {
    id: string;
    label: string;
    disabled?: boolean;
    shortcut?: string;
    onselect: () => void;
  };

  let root = $state<HTMLElement>();
  let place = $state<{ x: number; y: number } | null>(null);

  const items = $derived.by((): Entry[] => {
    if (!row) return [item("new-collection", "New collection", newCollection)];
    const target = row;
    const common = [
      item("rename", "Rename", () => {
        treeState.renaming = target.path;
      }),
      item("duplicate", "Duplicate", () => treeState.duplicate(target.path)),
      item("move", "Move to…", () => onmove(target)),
      shift("move-up", "Move up", target, -1, "Alt+ArrowUp"),
      shift("move-down", "Move down", target, 1, "Alt+ArrowDown"),
    ];
    const remove = item("delete", "Delete…", () => ondelete(target));
    if (target.kind === "request") return [...common, remove];
    return [
      item("new-request", "New request", () =>
        treeState.newRequest(target.path),
      ),
      item("new-folder", "New folder", () => treeState.newFolder(target.path)),
      ...common,
      item(
        "settings",
        target.depth === 0 ? "Collection settings" : "Folder settings",
        () => openSettings(target),
      ),
      remove,
    ];
  });

  function item(
    id: string,
    label: string,
    run: () => void | Promise<void>,
  ): Entry {
    return { id, label, onselect: () => void run() };
  }

  /** Moves the item one place in its folder, as `chord` on the tree does. */
  function shift(
    id: string,
    label: string,
    target: TreeRowData,
    by: -1 | 1,
    chord: string,
  ): Entry {
    return {
      ...item(id, label, () => treeState.shift(target.path, by)),
      disabled: !treeState.canShift(target.path, by),
      shortcut: formatChordText(chord, currentPlatform()) ?? undefined,
    };
  }

  function newCollection(): Promise<void> {
    return treeState.newFolder(ROOT);
  }

  function openSettings(target: TreeRowData): void {
    const shell = shellApi();
    const id = shell.findTab(
      "requests.folder",
      (params) => (params as { path?: string } | null)?.path === target.path,
    );
    if (id !== undefined) shell.activateTab(id);
    else shell.openTab("requests.folder", target.name, { path: target.path });
  }

  function setOpen(value: boolean): void {
    open = value;
    if (value) return;
    // Menu has put the focus on its button by the time this runs.
    queueMicrotask(() => {
      if (root?.contains(document.activeElement)) onclose?.();
    });
  }

  // Under the button, or where the person clicked; kept inside the window.
  $effect(() => {
    if (!open || !root) {
      place = null;
      return;
    }
    const button = root.getBoundingClientRect();
    const want = at ?? { x: button.left, y: button.bottom + 4 };
    const menu = root.querySelector<HTMLElement>("[role='menu']");
    const height = menu?.offsetHeight ?? 0;
    place = {
      x: want.x,
      y: Math.max(4, Math.min(want.y, window.innerHeight - height - 4)),
    };
  });
</script>

<div
  class="tree-menu"
  class:hidden
  bind:this={root}
  style:--menu-x={place ? `${place.x}px` : undefined}
  style:--menu-y={place ? `${place.y}px` : undefined}
>
  <Menu
    bind:open={() => open, setOpen}
    {items}
    menuLabel={row?.name ?? "Collections"}
    title="More actions"
  >
    {#snippet trigger()}
      <Ellipsis size={14} strokeWidth={1.5} />
    {/snippet}
  </Menu>
</div>

<style>
  .tree-menu {
    display: inline-flex;
  }

  .tree-menu :global(.trigger) {
    width: 24px;
    height: 24px;
    padding: 0;
    justify-content: center;
  }

  .tree-menu :global([role="menu"]) {
    position: fixed;
    left: var(--menu-x);
    top: var(--menu-y);
  }

  /* Out of the sidebar's flow, so opening it moves nothing. */
  .hidden {
    position: absolute;
  }

  .hidden :global(.trigger) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
