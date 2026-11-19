// Secret names in the command palette. Only names: the rows never hold a
// value, and nothing here asks the Vault for one (F1-FON-11 #7).

import { projectApi } from "$app/contributions/project-api";
import { shellApi } from "$app/contributions/shell-api";
import type { PaletteItem } from "$app/contributions/types";
import { vaultSecrets } from "./api";
import { secrets } from "./state/secrets.svelte";

export default async function secretNames(): Promise<PaletteItem[]> {
  const info = projectApi().info;
  if (!info) return [];
  const production = (name: string) =>
    // An environment the project no longer has counts as production.
    info.environments.find((e) => e.name === name)?.production ?? true;
  const rows = await vaultSecrets();
  return rows
    .filter((r) => !r.missing)
    .map((r) => ({
      id: `${r.environment}/${r.name}`,
      title: r.name,
      meta: "Name only, values never show here",
      environment: {
        name: r.environment,
        production: production(r.environment),
      },
      open: async () => {
        secrets.show(r.environment, r.name);
        await shellApi().openModule("vault");
      },
    }));
}
