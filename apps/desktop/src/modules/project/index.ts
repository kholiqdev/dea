import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "project",
  title: "Welcome",
  scope: "app",
  view: () => import("./WelcomeView.svelte"),
  statusItems: [
    {
      id: "project.environment",
      align: "start",
      order: 1,
      view: () => import("./components/EnvironmentStatus.svelte"),
    },
  ],
  commands: [
    {
      id: "project.open",
      title: "Show Welcome",
      action: { type: "openModule", module: "project" },
    },
  ],
};
