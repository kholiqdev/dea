import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "vault",
  title: "Vault",
  rail: { icon: "key-round", group: "bottom", order: 1 },
  view: () => import("./VaultView.svelte"),
  sidebar: () => import("./components/VaultSidebar.svelte"),
  statusItems: [
    {
      id: "vault.status",
      align: "end",
      order: 0,
      view: () => import("./components/VaultStatusItem.svelte"),
    },
  ],
  commands: [
    {
      id: "vault.open",
      title: "Open Vault",
      action: { type: "openModule", module: "vault" },
    },
    {
      id: "vault.lock",
      title: "Lock Vault",
      action: { type: "intent", intent: "vault.lock" },
    },
  ],
  shortcuts: [{ command: "vault.lock", chord: "Mod+L" }],
  intents: [{ intent: "vault.lock", handler: () => import("./intents/lock") }],
  searchSources: [
    {
      id: "vault.secrets",
      group: "Secret names",
      order: 70,
      source: () => import("./search"),
    },
  ],
};
