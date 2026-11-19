<!-- The environment request tabs use, in the title bar (F1-FON-06 #5). -->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Menu, { type MenuItem } from "@dea/ui/Menu.svelte";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import { runAction } from "./hotkeys.svelte";
  import { environmentColor } from "../contributions/project-api";
  import { effectiveShortcuts } from "../contributions/shortcuts";
  import { shellContext } from "./context";
  import { menus } from "./menus.svelte";

  const ctx = shellContext();
  const info = $derived(ctx.project.info);
  const active = $derived(ctx.project.environment);

  const items = $derived.by((): MenuItem[] => {
    const envs: MenuItem[] = (info?.environments ?? []).map((env) => ({
      id: `env:${env.name}`,
      label: env.name,
      checked: env.name === active?.name,
      dot: environmentColor(env),
      ...(env.production ? { badge: "production" } : {}),
      onselect: () => void ctx.project.setEnvironment(env.name),
    }));
    const edit = ctx.registry.commands.find(
      (c) => c.id === "project.editEnvironments",
    );
    if (edit) {
      const chord = effectiveShortcuts(ctx.registry, settings.shortcuts).find(
        (b) => b.command === edit.id,
      );
      envs.push({
        id: "edit",
        label: "Edit environments…",
        ...(chord
          ? { shortcut: formatChordText(chord.chord, currentPlatform()) ?? "" }
          : {}),
        onselect: () => void runAction(edit.action, ctx),
      });
    }
    return envs;
  });
</script>

<!-- Shown without environments too: it holds "Edit environments…". -->
{#if info}
  <Menu
    bind:open={menus.environment}
    {items}
    menuLabel="Environments"
    title="Environment for requests"
  >
    {#snippet trigger()}
      {#if active}
        <span class="dot" style:background={environmentColor(active)}></span>
        <span class="name">{active.name}</span>
        {#if active.production}<span class="prod-label">production</span>{/if}
      {:else}
        <span class="name none">No environment</span>
      {/if}
      <ChevronDown size={14} strokeWidth={1.5} />
    {/snippet}
  </Menu>
{/if}

<style>
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 4px;
  }

  .name {
    color: var(--text-1);
  }

  .none {
    color: var(--text-3);
  }

  .prod-label {
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 500 11px/1 var(--font-mono);
    color: var(--negative-text);
    box-shadow: inset 0 0 0 1px var(--negative);
  }
</style>
