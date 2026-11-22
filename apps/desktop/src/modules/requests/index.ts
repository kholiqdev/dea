import { methodColor, methodLabel } from "$lib/http/method";
import { FILTER_CHORD } from "./components/response/filter-layout";
import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "requests",
  title: "Requests",
  rail: { icon: "arrow-left-right", group: "main", order: 1 },
  view: () => import("./RequestsView.svelte"),
  sidebarHeader: "module",
  sidebarWhenNarrow: "hide",
  sidebar: () => import("./components/CollectionsSidebar.svelte"),
  tabTypes: [
    {
      id: "requests.request",
      title: "Request",
      icon: "arrow-left-right",
      environment: "window",
      badge: (params) => {
        const method = (params as { method?: string } | null)?.method;
        return method
          ? { text: methodLabel(method), color: methodColor(method) }
          : null;
      },
      view: () => import("./RequestTab.svelte"),
    },
    {
      id: "requests.folder",
      title: "Folder",
      icon: "arrow-left-right",
      environment: "window",
      view: () => import("./FolderTab.svelte"),
    },
    {
      id: "requests.cookies",
      title: "Cookies",
      icon: "cookie",
      view: () => import("./CookiesTab.svelte"),
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
    {
      id: "requests.send",
      title: "Send request",
      action: { type: "intent", intent: "requests.send" },
      scope: "project",
    },
    {
      id: "requests.focusFilter",
      title: "Filter response with JSONPath",
      action: { type: "intent", intent: "requests.focusFilter" },
      scope: "project",
    },
    {
      id: "requests.toggleLayout",
      title: "Toggle response position",
      action: { type: "intent", intent: "requests.toggleLayout" },
    },
    {
      id: "requests.manageCookies",
      title: "Manage cookies",
      action: { type: "intent", intent: "requests.manageCookies" },
      scope: "project",
    },
  ],
  shortcuts: [
    { command: "requests.open", chord: "Mod+1" },
    { command: "requests.send", chord: "Mod+Enter" },
    { command: "requests.focusFilter", chord: FILTER_CHORD },
  ],
  searchSources: [
    {
      id: "requests.search",
      group: "Requests",
      order: 10,
      source: () => import("./search"),
    },
  ],
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
    {
      intent: "requests.send",
      handler: () => import("./intents/send"),
    },
    {
      intent: "requests.focusFilter",
      handler: () => import("./intents/focus-filter"),
    },
    {
      intent: "requests.toggleLayout",
      handler: () => import("./intents/toggle-layout"),
    },
    {
      intent: "requests.manageCookies",
      handler: () => import("./intents/manage-cookies"),
    },
  ],
};
