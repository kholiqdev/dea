import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "mobile",
  title: "Mobile",
  rail: { icon: "smartphone", group: "main", order: 4 },
  view: () => import("./MobileView.svelte"),
  commands: [
    {
      id: "mobile.open",
      title: "Open Mobile",
      action: { type: "openModule", module: "mobile" },
    },
  ],
  shortcuts: [{ command: "mobile.open", chord: "Mod+4" }],
};
