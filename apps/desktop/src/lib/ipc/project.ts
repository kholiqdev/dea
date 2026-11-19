// The project of this window and its environments. One wrapper per command.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { EnvReference } from "./bindings/EnvReference";
import type { Environment } from "./bindings/Environment";
import type { ProjectChanged } from "./bindings/ProjectChanged";
import type { ProjectInfo } from "./bindings/ProjectInfo";
import type { RecentEntry } from "./bindings/RecentEntry";

export function windowProject(): Promise<ProjectInfo | null> {
  return invoke<ProjectInfo | null>("window_project");
}

export function openProject(root: string): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("open_project", { root });
}

export function createProject(
  location: string,
  name: string,
): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("create_project", { location, name });
}

/** A new window on the project at `root`, or on Welcome. */
export function openWindow(root: string | null): Promise<void> {
  return invoke<void>("open_window", { root });
}

export function closeProject(): Promise<void> {
  return invoke<void>("close_project");
}

export function pickFolder(): Promise<string | null> {
  return invoke<string | null>("pick_folder");
}

export function recentProjects(): Promise<RecentEntry[]> {
  return invoke<RecentEntry[]>("recent_projects");
}

export function removeRecent(root: string): Promise<void> {
  return invoke<void>("remove_recent", { root });
}

export function saveEnvironment(
  previous: string | null,
  environment: Environment,
): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("save_environment", { previous, environment });
}

export function deleteEnvironment(name: string): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("delete_environment", { name });
}

export function duplicateEnvironment(name: string): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("duplicate_environment", { name });
}

export function environmentReferences(name: string): Promise<EnvReference[]> {
  return invoke<EnvReference[]>("environment_references", { name });
}

export function setActiveEnvironment(name: string | null): Promise<void> {
  return invoke<void>("set_active_environment", { name });
}

export function clearLocalData(): Promise<void> {
  return invoke<void>("clear_local_data");
}

/** Calls `handler` each time files of this window's project change on disk. */
export function onProjectChanged(
  handler: (change: ProjectChanged) => void,
): Promise<() => void> {
  return getCurrentWebviewWindow().listen<ProjectChanged>(
    "project-changed",
    (e) => handler(e.payload),
  );
}
