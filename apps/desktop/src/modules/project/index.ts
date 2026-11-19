import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "project",
  title: "Welcome",
  scope: "app",
  view: () => import("./WelcomeView.svelte"),
  tabTypes: [
    {
      id: "project.environments",
      title: "Environments",
      icon: "layers",
      view: () => import("./EnvironmentsTab.svelte"),
    },
  ],
  statusItems: [
    {
      id: "project.watching",
      align: "start",
      order: 0,
      view: () => import("./components/WatchingStatus.svelte"),
    },
    {
      id: "project.environment",
      align: "start",
      order: 1,
      view: () => import("./components/EnvironmentStatus.svelte"),
    },
  ],
  commands: [
    {
      id: "project.editEnvironments",
      title: "Edit environments",
      action: { type: "intent", intent: "project.editEnvironments" },
    },
    {
      id: "project.saveEnvironments",
      title: "Save environments",
      action: { type: "intent", intent: "project.saveEnvironments" },
    },
    {
      id: "project.open",
      title: "Show Welcome",
      action: { type: "openModule", module: "project" },
    },
  ],
  shortcuts: [{ command: "project.saveEnvironments", chord: "Mod+S" }],
  searchSources: [
    {
      id: "project.searchEnvironments",
      group: "Environments",
      order: 60,
      source: () => import("./search"),
    },
  ],
  intents: [
    {
      intent: "project.editEnvironments",
      handler: () => import("./intents/edit-environments"),
    },
    {
      intent: "project.saveEnvironments",
      handler: () => import("./intents/save-environments"),
    },
  ],
};
