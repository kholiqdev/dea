import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "settings",
  title: "Settings",
  rail: { icon: "sliders-horizontal", group: "bottom", order: 2 },
  view: () => import("./SettingsView.svelte"),
  commands: [
    {
      id: "settings.open",
      title: "Open Settings",
      action: { type: "openModule", module: "settings" },
    },
  ],
  shortcuts: [{ command: "settings.open", chord: "Mod+," }],
};
