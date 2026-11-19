// Start-up: read the settings and the saved layout before the first frame,
// so the window opens in the right theme and the same shape as last time.

import { mount } from "svelte";
import { systemPrefersDark } from "@dea/ui/theme";
import { getUiState } from "$lib/ipc/settings";
import { settings } from "$lib/settings/settings.svelte";
import App from "./App.svelte";
import { registry } from "./contributions/registry.svelte";
import { exposeE2eApi, isE2e } from "./e2e";
import { registerBuiltInModules } from "./modules";
import { applyAppearance } from "./shell/appearance";
import { NO_PROJECT, shell } from "./shell/shell-state.svelte";
import { shellModule } from "./shell/shell-module";
import { tabs } from "./tabs/tabs.svelte";

/** The module shown when no project is open. */
const WELCOME = "project";

export async function start(
  target: HTMLElement,
): Promise<ReturnType<typeof mount>> {
  registerBuiltInModules(registry);
  registry.register(shellModule(shell, tabs));

  const [, saved] = await Promise.all([
    settings.load(),
    getUiState(NO_PROJECT).catch(() => null),
  ]);
  if (saved) {
    shell.restore(saved);
    tabs.restore(saved);
  }
  applyAppearance(
    document.documentElement,
    settings.appearance,
    systemPrefersDark(),
  );

  const app = mount(App, { target });
  if (isE2e()) exposeE2eApi();
  if (saved?.activeModule) await shell.openModule(saved.activeModule);
  if (!shell.activeModule) await shell.openModule(WELCOME);
  return app;
}
