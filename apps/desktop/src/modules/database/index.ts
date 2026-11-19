import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "database",
  title: "Database",
  rail: { icon: "database", group: "main", order: 2 },
  view: () => import("./DatabaseView.svelte"),
  commands: [
    {
      id: "database.open",
      title: "Open Database",
      action: { type: "openModule", module: "database" },
    },
  ],
  shortcuts: [{ command: "database.open", chord: "Mod+2" }],
};
