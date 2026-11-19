<!-- The project's name in the title bar, with what can be done to it. -->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Menu, { type MenuItem } from "@dea/ui/Menu.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import { closeProject, openWindow, pickFolder } from "$lib/ipc/project";
  import { toasts } from "../prompts/toasts.svelte";
  import { shellContext } from "./context";

  const ctx = shellContext();
  const info = $derived(ctx.project.info);

  function attempt(action: () => Promise<unknown>): void {
    action().catch((e: unknown) => toasts.show(errorMessage(e)));
  }

  const items = $derived.by((): MenuItem[] => [
    {
      id: "new",
      label: "New project…",
      onselect: () => attempt(() => openWindow(null)),
    },
    {
      id: "open",
      label: "Open project folder…",
      onselect: () =>
        attempt(async () => {
          const root = await pickFolder();
          if (root) await openWindow(root);
        }),
    },
    {
      id: "window",
      label: "New window",
      onselect: () => attempt(() => openWindow(info?.root ?? null)),
    },
    {
      id: "close",
      label: "Close project",
      onselect: () =>
        attempt(async () => {
          await closeProject();
          // A fresh start shows Welcome with nothing left of the project.
          location.reload();
        }),
    },
  ]);
</script>

{#if info}
  <Menu {items} menuLabel="Project">
    {#snippet trigger()}
      <span class="name">{info.name}</span>
      <ChevronDown size={14} strokeWidth={1.5} />
    {/snippet}
  </Menu>
{/if}

<style>
  .name {
    font-weight: 500;
    color: var(--text-1);
  }
</style>
