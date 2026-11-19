// The shortcuts in effect: each command's manifest chords, unless the
// person's settings replace them.

import type { Registry } from "./registry.svelte";

export interface Binding {
  command: string;
  chord: string;
}

/**
 * Every command's chords: the manifest defaults, except where an override
 * replaces them (`null` turns the shortcut off).
 */
export function effectiveShortcuts(
  registry: Registry,
  overrides: Record<string, string | null | undefined>,
): Binding[] {
  const out: Binding[] = [];
  for (const command of registry.commands) {
    if (command.id in overrides) {
      const chord = overrides[command.id];
      if (chord) out.push({ command: command.id, chord });
      continue;
    }
    for (const s of registry.shortcuts)
      if (s.command === command.id)
        out.push({ command: command.id, chord: s.chord });
  }
  return out;
}
