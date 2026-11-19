// Which Welcome page is showing.

export type WelcomePage =
  | "start"
  | { new: { name: string; location: string } }
  | { notAProject: string };

export const welcome = $state<{ page: WelcomePage }>({ page: "start" });

/** The New project page, filled in or empty. */
export function newProjectPage(name = "", location = ""): WelcomePage {
  return { new: { name, location } };
}

const DAY = 24 * 60 * 60;

/** When a project was last opened, as Welcome says it. Times in seconds. */
export function relativeDay(lastOpened: number, now: number): string {
  const days = Math.floor((now - lastOpened) / DAY);
  if (days <= 0) return "Opened today";
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 14) return "Last week";
  // A one-off format, never kept or watched, so a plain Date is right here.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  return new Date(lastOpened * 1000).toLocaleDateString("en", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

/** The last part of a path, for a project name suggested from its folder. */
export function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "";
}
