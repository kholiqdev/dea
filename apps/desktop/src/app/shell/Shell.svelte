<!-- The window: title bar, rail, sidebar, work area and status bar. -->
<script lang="ts">
  import { settings } from "$lib/settings/settings.svelte";
  import type { Registry } from "../contributions/registry.svelte";
  import { setShellContext } from "./context";
  import { installHotkeys } from "./hotkeys.svelte";
  import Rail from "./Rail.svelte";
  import type { Shell } from "./shell-state.svelte";
  import Sidebar from "./Sidebar.svelte";
  import StatusBar from "./StatusBar.svelte";
  import TitleBar from "./TitleBar.svelte";
  import Workspace from "./Workspace.svelte";

  let { registry, shell }: { registry: Registry; shell: Shell } = $props();

  setShellContext({
    get registry() {
      return registry;
    },
    get shell() {
      return shell;
    },
  });

  const module = $derived(
    shell.activeModule ? registry.module(shell.activeModule) : undefined,
  );

  $effect(() =>
    installHotkeys({ registry, shell, overrides: () => settings.shortcuts }),
  );
</script>

<div class="app">
  <TitleBar />
  <div class="body">
    <Rail />
    {#if module?.sidebar && shell.sidebar(module.id).visible}
      <Sidebar {module} />
    {/if}
    <Workspace />
  </div>
  <StatusBar />
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
</style>
