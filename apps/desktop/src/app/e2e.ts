// Hooks for the end-to-end tests. The flag comes from an initialization
// script that only a debug build adds, and only with DEA_E2E=1.

import { openProject } from "$lib/ipc/project";
import { shellApi } from "./contributions/shell-api";

export interface E2eApi {
  openTab(title: string): string;
  /** Opens the project at `root` in this window, which then starts again. */
  openProject(root: string): Promise<void>;
}

declare global {
  interface Window {
    __DEA_E2E__?: boolean;
    __deaE2e?: E2eApi;
  }
}

export function isE2e(): boolean {
  return typeof window !== "undefined" && window.__DEA_E2E__ === true;
}

export function exposeE2eApi(): void {
  window.__deaE2e = {
    openTab: (title) => shellApi().openTab("e2e.tab", title, null),
    openProject: async (root) => {
      await openProject(root);
      location.reload();
    },
  };
}
