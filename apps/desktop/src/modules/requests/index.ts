import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "requests",
  title: "Requests",
  rail: { icon: "arrow-left-right", group: "main", order: 1 },
  view: () => import("./RequestsView.svelte"),
  sidebarHeader: "module",
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
    {
      id: "requests.newRequest",
      title: "New request",
      action: { type: "intent", intent: "requests.newRequest" },
    },
    {
      id: "requests.newCollection",
      title: "New collection",
      action: { type: "intent", intent: "requests.newCollection" },
    },
  ],
  shortcuts: [{ command: "requests.open", chord: "Mod+1" }],
  settings: [
    {
      id: "requests.settings",
      title: "Requests",
      order: 40,
      view: () => import("./components/RequestsSettings.svelte"),
    },
  ],
  intents: [
    {
      intent: "requests.newRequest",
      handler: () => import("./intents/new-request"),
    },
    {
      intent: "requests.newCollection",
      handler: () => import("./intents/new-collection"),
    },
  ],
};
