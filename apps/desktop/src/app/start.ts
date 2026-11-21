// Start-up: read the settings and the saved layout before the first frame,
// so the window opens in the right theme and the same shape as last time.

import { mount } from "svelte";
import { systemPrefersDark } from "@dea/ui/theme";
import { setActiveEnvironment } from "$lib/ipc/project";
import { getUiState } from "$lib/ipc/settings";
import { settings } from "$lib/settings/settings.svelte";
import App from "./App.svelte";
import { setProjectApi } from "./contributions/project-api";
import { registry } from "./contributions/registry.svelte";
import { setVaultApi } from "./contributions/vault-api";
import { exposeE2eApi, isE2e } from "./e2e";
import { registerBuiltInModules } from "./modules";
import { thisProject } from "./project/project.svelte";
import { errorMessage } from "$lib/ipc/errors";
import { sendConfirmations } from "./prompts/send-confirm.svelte";
import { toasts } from "./prompts/toasts.svelte";
import { vaultPrompts } from "./prompts/vault-prompt.svelte";
import { applyAppearance } from "./shell/appearance";
import { NO_PROJECT, shell } from "./shell/shell-state.svelte";
import { shellModule } from "./shell/shell-module";
import { tabs } from "./tabs/tabs.svelte";
import { thisVault } from "./vault/vault.svelte";

/** The module shown when no project is open. */
const WELCOME = "project";

export async function start(
  target: HTMLElement,
): Promise<ReturnType<typeof mount>> {
  registerBuiltInModules(registry);
  registry.register(shellModule(shell, tabs, thisProject));
  setProjectApi(thisProject);
  setVaultApi(thisVault);

  // Every window, Welcome too, follows the Vault and the settings and
  // answers the Vault's prompts. Each listens before it reads, so a change
  // made while the window starts is not missed.
  const [, info] = await Promise.all([
    settings
      .listen()
      .catch(() => {})
      .then(() => settings.load()),
    thisProject.load(),
    thisVault
      .listen()
      .catch(() => {})
      .then(() => thisVault.load()),
  ]);
  vaultPrompts.listen().catch(() => {});
  // Without its listener no confirmation shows, and a send would wait for one.
  sendConfirmations
    .listen()
    .catch((e: unknown) => toasts.show(errorMessage(e)));
  registry.setProjectOpen(info !== null);
  // Each window of a project keeps its own layout, tabs and environment.
  const scope = info?.scope ?? NO_PROJECT;
  const slot = info?.slot ?? 0;
  const saved = await getUiState(scope, slot).catch(() => null);
  if (saved) {
    shell.restore(saved);
    if (info) tabs.restore(saved);
    thisProject.restoreEnvironment(saved.activeEnvironment);
  }
  applyAppearance(
    document.documentElement,
    settings.appearance,
    systemPrefersDark(),
  );

  const app = mount(App, { target, props: { scope, slot } });
  if (isE2e()) exposeE2eApi();
  thisVault.trackActivity();
  if (info) {
    thisProject.listen().catch(() => {});
    thisProject.recheckOnFocus();
    setActiveEnvironment(thisProject.environment?.name ?? null).catch(() => {});
  }
  if (saved?.activeModule && registry.module(saved.activeModule))
    await shell.openModule(saved.activeModule);
  const first = info ? registry.modules[0]?.id : WELCOME;
  if (!shell.activeModule && first) await shell.openModule(first);
  return app;
}
