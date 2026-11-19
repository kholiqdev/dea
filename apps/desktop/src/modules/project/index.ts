import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "project",
  title: "Welcome",
  view: () => import("./WelcomeView.svelte"),
  commands: [
    {
      id: "project.open",
      title: "Show Welcome",
      action: { type: "openModule", module: "project" },
    },
  ],
};
