import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "requests",
  title: "Requests",
  rail: { icon: "arrow-left-right", group: "main", order: 1 },
  view: () => import("./RequestsView.svelte"),
  sidebar: () => import("./components/CollectionsSidebar.svelte"),
  tabTypes: [
    {
      id: "requests.request",
      title: "Request",
      icon: "arrow-left-right",
      environment: "window",
      view: () => import("./RequestTab.svelte"),
    },
    {
      id: "requests.folder",
      title: "Folder",
      icon: "arrow-left-right",
      environment: "window",
      view: () => import("./FolderTab.svelte"),
    },
  ],
  commands: [
    {
      id: "requests.open",
      title: "Open Requests",
      action: { type: "openModule", module: "requests" },
    },
  ],
  shortcuts: [{ command: "requests.open", chord: "Mod+1" }],
};
