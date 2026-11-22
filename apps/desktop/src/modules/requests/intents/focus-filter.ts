// Filter the response (Mod+Alt+F, the palette): puts the focus in the
// JSONPath filter of the request tab being shown.

import { shellApi } from "$app/contributions/shell-api";
import { focusFilter } from "../lib/filter-focus";

export default async function focusActiveFilter(): Promise<void> {
  const shell = shellApi();
  const tab = shell.activeTab();
  if (tab?.tabType !== "requests.request") {
    shell.toast("Open a request to filter its response.");
    return;
  }
  if (!(await focusFilter(tab.id))) {
    shell.toast("There is no JSON response to filter.");
  }
}
