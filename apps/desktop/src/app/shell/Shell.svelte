<!-- The window: title bar, rail, sidebar, work area and status bar. -->
<script lang="ts">
  import { settings } from "$lib/settings/settings.svelte";
  import type { ProjectApi } from "../contributions/project-api";
  import type { Registry } from "../contributions/registry.svelte";
  import ConfirmClose from "../prompts/ConfirmClose.svelte";
  import { closeQuestion } from "../prompts/confirm-close.svelte";
  import ToastStack from "../prompts/ToastStack.svelte";
  import type { Tabs } from "../tabs/tabs.svelte";
  import { setShellContext } from "./context";
  import { createHotkeyHandler } from "./hotkeys.svelte";
  import Rail from "./Rail.svelte";
  import type { Shell } from "./shell-state.svelte";
  import Sidebar from "./Sidebar.svelte";
  import StatusBar from "./StatusBar.svelte";
  import TitleBar from "./TitleBar.svelte";
  import Workspace from "./Workspace.svelte";

  let {
    registry,
    shell,
    tabs,
    project,
  }: { registry: Registry; shell: Shell; tabs: Tabs; project: ProjectApi } =
    $props();

  const ctx = setShellContext({
    get registry() {
      return registry;
    },
    get shell() {
      return shell;
    },
    get tabs() {
      return tabs;
    },
    get project() {
      return project;
    },
  });

  // Without a project the window shows Welcome: no rail, no tabs.
  const welcome = $derived(!project.info);

  const onHotkey = createHotkeyHandler(ctx, () => settings.shortcuts);

  const module = $derived(
    shell.activeModule ? registry.module(shell.activeModule) : undefined,
  );
</script>

<svelte:window onkeydowncapture={onHotkey} />

<div
  class="app"
  data-env={project.environment?.production ? "production" : undefined}
>
  <TitleBar />
  <div class="body">
    {#if !welcome}<Rail />{/if}
    {#if module?.sidebar && shell.sidebar(module.id).visible}
      <Sidebar {module} />
    {/if}
    <Workspace />
  </div>
  <StatusBar />
</div>
<ConfirmClose question={closeQuestion} />
<ToastStack />

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
