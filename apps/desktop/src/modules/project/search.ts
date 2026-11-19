// The project's environments in the command palette: choosing one makes it
// the environment requests use.

import { projectApi } from "$app/contributions/project-api";
import { shellApi } from "$app/contributions/shell-api";
import type { PaletteItem } from "$app/contributions/types";

export default function environments(): PaletteItem[] {
  const project = projectApi();
  return (project.info?.environments ?? []).map((env) => ({
    id: env.name,
    title: env.name,
    meta: "Use for requests",
    environment: { name: env.name, production: env.production },
    open: async () => {
      // The palette lists what was there when it opened.
      if (!project.info?.environments.some((e) => e.name === env.name)) {
        shellApi().toast(`${env.name} no longer exists`);
        return;
      }
      await project.setEnvironment(env.name);
    },
  }));
}
