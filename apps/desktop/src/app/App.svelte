<script lang="ts">
  import { systemPrefersDark, watchSystemTheme } from "@dea/ui/theme";
  import { settings } from "$lib/settings/settings.svelte";
  import { registry } from "./contributions/registry.svelte";
  import { setShellApi } from "./contributions/shell-api";
  import { applyAppearance } from "./shell/appearance";
  import Shell from "./shell/Shell.svelte";
  import {
    NO_PROJECT,
    persistUiState,
    shell,
  } from "./shell/shell-state.svelte";
  import { tabs } from "./tabs/tabs.svelte";

  // What modules may ask of the shell.
  setShellApi({
    openTab: (tabType, title, params, opts) =>
      tabs.open(tabType, title, params, opts),
    closeTab: (id) => tabs.close(id),
    setTabDirty: (id, dirty) => tabs.setDirty(id, dirty),
    setSaveHandler: (tabType, save) => tabs.setSaveHandler(tabType, save),
    openModule: (id) => shell.openModule(id),
  });

  let systemDark = $state(systemPrefersDark());

  // "Match system" follows the OS as it changes.
  $effect(() => watchSystemTheme((dark) => (systemDark = dark)));
  $effect(() =>
    applyAppearance(document.documentElement, settings.appearance, systemDark),
  );
  $effect(() =>
    persistUiState(NO_PROJECT, () => ({
      ...shell.snapshot(),
      ...tabs.snapshot(),
    })),
  );
</script>

<Shell {registry} {shell} {tabs} />
