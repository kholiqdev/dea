// Settings opened on one section, as the command palette asks.

import { sections } from "../state/sections.svelte";

export default function openSection(args: unknown): void {
  const section = (args as { section?: unknown } | undefined)?.section;
  if (typeof section === "string") sections.current = section;
}
