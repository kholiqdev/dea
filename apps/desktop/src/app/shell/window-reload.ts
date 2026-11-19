// Starting this window again, for a project it now has or no longer has.
// Tests replace `run`, since jsdom cannot navigate.

export const windowReload = {
  run: (): void => location.reload(),
};
