<script lang="ts">
  import { MediaQuery } from "svelte/reactivity";
  import { settings } from "$lib/settings/settings.svelte";
  import { registry } from "./contributions/registry.svelte";
  import { setShellApi } from "./contributions/shell-api";
  import { runAction } from "./shell/hotkeys.svelte";
  import { applyAppearance } from "./shell/appearance";
  import Shell from "./shell/Shell.svelte";
  import { persistUiState, shell } from "./shell/shell-state.svelte";
  import { thisProject } from "./project/project.svelte";
  import { toasts } from "./prompts/toasts.svelte";
  import { tabs } from "./tabs/tabs.svelte";

  let { scope, slot }: { scope: string; slot: number } = $props();

  // What modules may ask of the shell.
  setShellApi({
    openTab: (tabType, title, params, opts) =>
      tabs.open(tabType, title, params, opts),
    findTab: (tabType, match) => tabs.find(tabType, match),
    updateTab: (id, patch) => tabs.update(id, patch),
    activateTab: (id) => {
      if (!tabs.tabs.some((t) => t.id === id)) return false;
      tabs.activate(id);
      return true;
    },
    closeTab: (id) => tabs.close(id),
    setTabDirty: (id, dirty) => tabs.setDirty(id, dirty),
    setSaveHandler: (tabType, save) => tabs.setSaveHandler(tabType, save),
    openModule: (id) => shell.openModule(id),
    runCommand: async (id) => {
      const command = registry.commands.find((c) => c.id === id);
      if (command) await runAction(command.action, { registry, shell });
    },
    toast: (text, opts) => toasts.show(text, opts),
    dismissToast: (id) => toasts.dismiss(id),
  });

  // "Match system" follows the OS as it changes.
  const systemDark = new MediaQuery("(prefers-color-scheme: dark)");

  // The theme, fonts and size live on <html>, outside this component.
  $effect(() =>
    applyAppearance(
      document.documentElement,
      settings.appearance,
      systemDark.current,
    ),
  );
  $effect(() =>
    persistUiState(scope, slot, () => ({
      ...shell.snapshot(),
      ...tabs.snapshot(),
      activeEnvironment: thisProject.environment?.name ?? null,
    })),
  );
</script>

<Shell {registry} {shell} {tabs} project={thisProject} />
