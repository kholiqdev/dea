import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "settings",
  title: "Settings",
  scope: "app",
  rail: { icon: "sliders-horizontal", group: "bottom", order: 2 },
  view: () => import("./SettingsView.svelte"),
  sidebar: () => import("./SettingsSidebar.svelte"),
  commands: [
    {
      id: "settings.open",
      title: "Open Settings",
      action: { type: "openModule", module: "settings" },
    },
  ],
  shortcuts: [{ command: "settings.open", chord: "Mod+," }],
  settings: [
    {
      id: "settings.appearance",
      title: "Appearance",
      order: 1,
      view: () => import("./components/AppearanceSection.svelte"),
    },
    {
      id: "settings.shortcuts",
      title: "Keyboard shortcuts",
      order: 2,
      view: () => import("./components/ShortcutsSection.svelte"),
    },
  ],
};
