import type { ModuleManifest } from "$app/contributions/types";

export const manifest: ModuleManifest = {
  id: "terminal",
  title: "Terminal",
  rail: { icon: "square-terminal", group: "main", order: 3 },
  view: () => import("./TerminalView.svelte"),
  commands: [
    {
      id: "terminal.open",
      title: "Open Terminal",
      action: { type: "openModule", module: "terminal" },
    },
  ],
  shortcuts: [{ command: "terminal.open", chord: "Mod+3" }],
};
