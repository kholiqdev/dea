<!-- The window: title bar, rail, sidebar, work area and status bar. -->
<script lang="ts">
  import { MediaQuery } from "svelte/reactivity";
  import { settings } from "$lib/settings/settings.svelte";
  import type { ProjectApi } from "../contributions/project-api";
  import type { Registry } from "../contributions/registry.svelte";
  import Palette from "../palette/Palette.svelte";
  import ConfirmClose from "../prompts/ConfirmClose.svelte";
  import { closeQuestion } from "../prompts/confirm-close.svelte";
  import { sendConfirmations } from "../prompts/send-confirm.svelte";
  import SendToProduction from "../prompts/SendToProduction.svelte";
  import VaultPrompt from "../prompts/VaultPrompt.svelte";
  import { vaultPrompts } from "../prompts/vault-prompt.svelte";
  import ToastStack from "../prompts/ToastStack.svelte";
  import type { Tabs } from "../tabs/tabs.svelte";
  import { setShellContext } from "./context";
  import { createHotkeyHandler } from "./hotkeys.svelte";
  import { menus } from "./menus.svelte";
  import Rail from "./Rail.svelte";
  import { NARROW_WINDOW, type Shell } from "./shell-state.svelte";
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

  const narrow = new MediaQuery(`(max-width: ${NARROW_WINDOW}px)`);
  $effect.pre(() => {
    shell.narrow = narrow.current;
  });

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
    {#if module?.sidebar && shell.sidebarShown(module.id)}
      <Sidebar {module} />
    {/if}
    <Workspace />
  </div>
  <StatusBar />
</div>
<ConfirmClose question={closeQuestion} />
{#if menus.palette}<Palette />{/if}
<VaultPrompt prompts={vaultPrompts} />
<SendToProduction confirmations={sendConfirmations} />
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
