<!-- The project's name in the title bar, with what can be done to it. -->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Menu, { type MenuItem } from "@dea/ui/Menu.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import { closeProject, openWindow, pickFolder } from "$lib/ipc/project";
  import ClearLocalData from "../prompts/ClearLocalData.svelte";
  import { clearLocalData } from "../prompts/clear-local-data";
  import { toasts } from "../prompts/toasts.svelte";
  import { blockedByUnsaved } from "../prompts/unsaved";
  import { shellContext } from "./context";
  import { menus } from "./menus.svelte";
  import { windowReload } from "./window-reload";

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
      id: "clear",
      label: "Clear local data…",
      onselect: () => (menus.clearLocalData = true),
    },
    {
      id: "close",
      label: "Close project",
      onselect: () =>
        attempt(async () => {
          if (blockedByUnsaved(ctx.tabs)) return;
          await closeProject();
          // A fresh start shows Welcome with nothing left of the project.
          windowReload.run();
        }),
    },
  ]);
</script>

{#if info}
  <Menu {items} menuLabel="Project" title="Project">
    {#snippet trigger()}
      <span class="name">{info.name}</span>
      <ChevronDown size={14} strokeWidth={1.5} />
    {/snippet}
  </Menu>
  {#if menus.clearLocalData}
    <ClearLocalData
      name={info.name}
      oncancel={() => (menus.clearLocalData = false)}
      onclear={() => {
        menus.clearLocalData = false;
        attempt(() => clearLocalData(ctx.shell, ctx.tabs));
      }}
    />
  {/if}
{/if}

<style>
  .name {
    font-weight: 500;
    color: var(--text-1);
  }
</style>
