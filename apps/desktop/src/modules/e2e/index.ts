// A tab type for the end-to-end tests, registered only when a debug build
// runs with DEA_E2E=1. A release build never registers it.

import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "e2e",
  title: "E2E",
  scope: "app",
  tabTypes: [
    {
      id: "e2e.tab",
      title: "Test tab",
      icon: "square-terminal",
      view: () => import("./E2eTabView.svelte"),
    },
  ],
};
