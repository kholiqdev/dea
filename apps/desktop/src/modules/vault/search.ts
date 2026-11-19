// Secret names in the command palette: the names `vault.yaml` declares.
// Listing them needs no IPC, so opening the palette never opens the Vault,
// touches the keychain or reads a value (F1-FON-11 #7).

import { projectApi } from "$app/contributions/project-api";
import { shellApi } from "$app/contributions/shell-api";
import type { PaletteItem } from "$app/contributions/types";
import { secrets } from "./state/secrets.svelte";

export default function secretNames(): PaletteItem[] {
  const info = projectApi().info;
  return (info?.secretNames ?? []).map((name) => ({
    id: name,
    title: name,
    meta: "Name only, values never show here",
    open: async () => {
      const now = projectApi().info;
      // The palette lists what was there when it opened.
      if (!now?.secretNames.includes(name)) {
        shellApi().toast(`${name} no longer exists`);
        return;
      }
      // Shown in the environment the Vault screen would show.
      const environment = now ? secrets.current(now) : null;
      if (environment) secrets.show(environment, name);
      await shellApi().openModule("vault");
    },
  }));
}
