// The project module's calls to Dea.

export {
  createProject,
  deleteEnvironment,
  duplicateEnvironment,
  environmentReferences,
  openProject,
  pickFolder,
  recentProjects,
  removeRecent,
  saveEnvironment,
} from "$lib/ipc/project";

/** Window actions the tests can replace. */
export const windowActions = {
  /** Starts this window again on the project it now has. */
  reload: (): void => location.reload(),
};
