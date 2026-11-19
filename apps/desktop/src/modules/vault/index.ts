import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "vault",
  title: "Vault",
  rail: { icon: "key-round", group: "bottom", order: 1 },
  view: () => import("./VaultView.svelte"),
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
  ],
};
