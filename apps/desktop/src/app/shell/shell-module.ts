// The shell's own commands, registered like any module's (without a rail
// item), so they show up in Settings and can be rebound.

import { openWindow } from "$lib/ipc/project";
import type { ProjectApi } from "../contributions/project-api";
import type { Action, ModuleManifest } from "../contributions/types";
import { menus } from "./menus.svelte";
import type { Tabs } from "../tabs/tabs.svelte";
import type { Shell } from "./shell-state.svelte";

interface ShellCommand {
  id: string;
  title: string;
  chord: string;
  run: () => unknown;
}

export function shellModule(
  shell: Shell,
  tabs: Tabs,
  project: ProjectApi,
): ModuleManifest {
  const commands: ShellCommand[] = [
    {
      id: "shell.toggleSidebar",
      title: "Show or hide the sidebar",
      chord: "Mod+B",
      run: () => {
        if (shell.activeModule) shell.toggleSidebar(shell.activeModule);
      },
    },
    {
      id: "shell.chooseEnvironment",
      title: "Choose the environment for requests",
      chord: "Mod+E",
      run: () => {
        if (project.info) menus.environment = true;
      },
    },
    {
      id: "shell.newWindow",
      title: "New window",
      chord: "Mod+Shift+N",
      run: () => openWindow(project.info?.root ?? null),
    },
    {
      id: "shell.closeTab",
      title: "Close tab",
      chord: "Mod+W",
      run: () => (tabs.active ? tabs.close(tabs.active.id) : undefined),
    },
    {
      id: "shell.reopenTab",
      title: "Reopen closed tab",
      chord: "Mod+Shift+T",
      run: () => tabs.reopenLast(),
    },
    {
      id: "shell.moveTabLeft",
      title: "Move tab left",
      chord: "Mod+Shift+PageUp",
      run: () => tabs.moveActive(-1),
    },
    {
      id: "shell.moveTabRight",
      title: "Move tab right",
      chord: "Mod+Shift+PageDown",
      run: () => tabs.moveActive(1),
    },
  ];
  const action = (id: string): Action => ({ type: "intent", intent: id });
  return {
    id: "shell",
    title: "Shell",
    scope: "app",
    commands: commands.map((c) => ({
      id: c.id,
      title: c.title,
      action: action(c.id),
    })),
    shortcuts: commands.map((c) => ({ command: c.id, chord: c.chord })),
    intents: commands.map((c) => ({
      intent: c.id,
      handler: () => Promise.resolve({ default: c.run }),
    })),
  };
}
