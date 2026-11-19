// The shell's own commands, registered like any module's (without a rail
// item), so they show up in Settings and can be rebound.

import type { ModuleManifest } from "../contributions/types";
import type { Shell } from "./shell-state.svelte";

function handler(run: () => void) {
  return () => Promise.resolve({ default: run });
}

export function shellModule(shell: Shell): ModuleManifest {
  return {
    id: "shell",
    title: "Shell",
    commands: [
      {
        id: "shell.toggleSidebar",
        title: "Show or hide the sidebar",
        action: { type: "intent", intent: "shell.toggleSidebar" },
      },
    ],
    shortcuts: [{ command: "shell.toggleSidebar", chord: "Mod+B" }],
    intents: [
      {
        intent: "shell.toggleSidebar",
        handler: handler(() => {
          if (shell.activeModule) shell.toggleSidebar(shell.activeModule);
        }),
      },
    ],
  };
}
