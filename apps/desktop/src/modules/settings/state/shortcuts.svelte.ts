// The Keyboard shortcuts page: each command's chord, and editing one with
// any clash shown before it is saved.

import { registry } from "$app/contributions/registry.svelte";
import { effectiveShortcuts } from "$app/contributions/shortcuts";
import { chordToString, parseChord } from "$lib/keyboard/chord";
import { findConflicts } from "$lib/keyboard/conflicts";
import { settings } from "$lib/settings/settings.svelte";

export interface ShortcutRow {
  command: string;
  title: string;
  /** The chord in effect, or null when it is off. */
  chord: string | null;
  /** Whether the person changed it. */
  overridden: boolean;
}

function canonical(text: string): string | null {
  try {
    return chordToString(parseChord(text));
  } catch {
    return null;
  }
}

/** Every command with the chord it has now. */
export function shortcutRows(): ShortcutRow[] {
  const bindings = effectiveShortcuts(registry, settings.shortcuts);
  return registry.commands.map((c) => {
    const chord = bindings.find((b) => b.command === c.id)?.chord ?? null;
    return {
      command: c.id,
      title: c.title,
      chord: chord && canonical(chord),
      overridden: c.id in settings.shortcuts,
    };
  });
}

function defaultChord(command: string): string | null {
  const chord = registry.shortcuts.find((s) => s.command === command)?.chord;
  return chord ? canonical(chord) : null;
}

export class ShortcutEditor {
  /** The command being changed. */
  command = $state<string | null>(null);
  /** The chord pressed so far, canonical. */
  captured = $state<string | null>(null);

  /** Commands that already use the captured chord. */
  clashes = $derived.by((): string[] => {
    const command = this.command;
    const captured = this.captured;
    if (!command || !captured) return [];
    const map: Record<string, string | null> = {};
    for (const row of shortcutRows()) map[row.command] = row.chord;
    map[command] = captured;
    const conflict = findConflicts(map).find((c) =>
      c.commands.includes(command),
    );
    return conflict?.commands.filter((c) => c !== command) ?? [];
  });

  start(command: string): void {
    this.command = command;
    this.captured = null;
  }

  cancel(): void {
    this.command = null;
    this.captured = null;
  }

  capture(chord: string): void {
    this.captured = canonical(chord);
  }

  get canSave(): boolean {
    return this.captured !== null && this.clashes.length === 0;
  }

  /** Saves the captured chord. Not while it clashes. */
  async save(): Promise<void> {
    if (!this.command || !this.canSave) return;
    await this.#set({ [this.command]: this.captured });
  }

  /** Saves the captured chord and turns it off for the commands using it. */
  async useHere(): Promise<void> {
    if (!this.command || !this.captured) return;
    const off = Object.fromEntries(this.clashes.map((c) => [c, null]));
    await this.#set({ ...off, [this.command]: this.captured });
  }

  async turnOff(): Promise<void> {
    if (!this.command) return;
    await this.#set({ [this.command]: null });
  }

  async #set(changes: Record<string, string | null>): Promise<void> {
    const next = { ...settings.shortcuts };
    for (const [command, chord] of Object.entries(changes)) {
      // Choosing the default again simply removes the change.
      if (chord !== null && chord === defaultChord(command))
        delete next[command];
      else next[command] = chord;
    }
    await settings.update({ shortcuts: next });
    this.cancel();
  }
}

export async function resetShortcut(command: string): Promise<void> {
  const next = { ...settings.shortcuts };
  delete next[command];
  await settings.update({ shortcuts: next });
}

export async function resetAllShortcuts(): Promise<void> {
  await settings.update({ shortcuts: {} });
}
