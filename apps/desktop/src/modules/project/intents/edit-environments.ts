// Shows the environment editor: the open tab, or a new one.

import { shellApi } from "$app/contributions/shell-api";

export default function editEnvironments(): void {
  const api = shellApi();
  const open = api.findTab("project.environments");
  if (open && api.activateTab(open)) return;
  api.openTab("project.environments", "Environments", null);
}
