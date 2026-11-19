import { chordToString, parseChord } from "./chord";

export interface Conflict {
  chord: string;
  commands: string[];
}

/**
 * Chords bound to more than one command. `null` means the shortcut is
 * turned off; text that is not a chord is ignored here.
 */
export function findConflicts(
  bindings: Record<string, string | null>,
): Conflict[] {
  const byChord = new Map<string, string[]>();
  for (const [command, text] of Object.entries(bindings)) {
    if (text === null) continue;
    let chord: string;
    try {
      chord = chordToString(parseChord(text));
    } catch {
      continue;
    }
    byChord.set(chord, [...(byChord.get(chord) ?? []), command]);
  }
  return [...byChord]
    .filter(([, commands]) => commands.length > 1)
    .map(([chord, commands]) => ({ chord, commands }));
}
