<script lang="ts">
  import { systemPrefersDark, watchSystemTheme } from "@dea/ui/theme";
  import { settings } from "$lib/settings/settings.svelte";
  import { registry } from "./contributions/registry.svelte";
  import { applyAppearance } from "./shell/appearance";
  import Shell from "./shell/Shell.svelte";
  import {
    NO_PROJECT,
    persistUiState,
    shell,
  } from "./shell/shell-state.svelte";

  let systemDark = $state(systemPrefersDark());

  // "Match system" follows the OS as it changes.
  $effect(() => watchSystemTheme((dark) => (systemDark = dark)));
  $effect(() =>
    applyAppearance(document.documentElement, settings.appearance, systemDark),
  );
  $effect(() => persistUiState(NO_PROJECT, () => shell.snapshot()));
</script>

<Shell {registry} {shell} />
