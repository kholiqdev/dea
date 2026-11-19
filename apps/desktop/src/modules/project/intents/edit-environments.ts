// Shows the environment editor: the open tab, or a new one that starts
// from the files (a closed tab's drafts were saved or let go).

import { shellApi } from "$app/contributions/shell-api";
import { resetEnvironmentsEditor } from "../state/environments.svelte";

export default function editEnvironments(): void {
  const api = shellApi();
  const open = api.findTab("project.environments");
  if (open && api.activateTab(open)) return;
  resetEnvironmentsEditor();
  api.openTab("project.environments", "Environments", null);
}
