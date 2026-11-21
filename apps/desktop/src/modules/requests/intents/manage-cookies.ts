// Manage cookies (the palette, the response's Cookies tab): shows the
// cookie manager, in the tab already open or else in a new one.

import { projectApi } from "$app/contributions/project-api";
import { shellApi } from "$app/contributions/shell-api";

export default function manageCookies(): Promise<void> {
  const shell = shellApi();
  const open = shell.findTab("requests.cookies");
  if (open !== undefined) {
    shell.activateTab(open);
    return Promise.resolve();
  }
  const environment = projectApi().environment?.name ?? null;
  shell.openTab("requests.cookies", "Cookies", { environment });
  return Promise.resolve();
}
