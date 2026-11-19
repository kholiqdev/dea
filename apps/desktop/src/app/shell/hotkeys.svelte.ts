// Keyboard shortcuts: a manifest's default chords, with the person's
// overrides from Settings on top.

import {
  chordFromEvent,
  currentPlatform,
  parseChord,
  sameChord,
  type Chord,
  type Platform,
} from "$lib/keyboard/chord";
import type { Registry } from "../contributions/registry.svelte";
import { effectiveShortcuts } from "../contributions/shortcuts";
import type { Action } from "../contributions/types";
import { toasts } from "../prompts/toasts.svelte";
import type { Shell } from "./shell-state.svelte";

export async function runAction(
  action: Action,
  ctx: { registry: Registry; shell: Shell },
): Promise<void> {
  switch (action.type) {
    case "openModule":
      return ctx.shell.openModule(action.module);
    case "openSettings": {
      await ctx.shell.openModule("settings");
      // The Settings module shows the section; the shell does not know it.
      const result = await ctx.registry.invoke("settings.openSection", {
        section: action.section,
      });
      if (!result.ok) toasts.show(result.message);
      return;
    }
    case "intent": {
      const result = await ctx.registry.invoke(action.intent, action.args);
      if (!result.ok) toasts.show(result.message);
    }
  }
}

function typingIn(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

/**
 * A keydown handler for `window` (capture phase) that runs the command
 * bound to the pressed chord. Call while a component initialises, so the
 * parsed shortcuts are owned by it.
 */
export function createHotkeyHandler(
  ctx: { readonly registry: Registry; readonly shell: Shell },
  overrides: () => Record<string, string | null | undefined>,
  platform: Platform = currentPlatform(),
): (e: KeyboardEvent) => void {
  const parsed = $derived(
    effectiveShortcuts(ctx.registry, overrides()).flatMap((b) => {
      try {
        return [{ command: b.command, chord: parseChord(b.chord) }];
      } catch {
        return [];
      }
    }),
  );

  return (e) => {
    // A dialog has the keyboard to itself until it closes.
    if (document.querySelector('[aria-modal="true"]')) return;
    // A shortcut recorder, such as the one in Settings, gets every key.
    if (e.target instanceof Element && e.target.closest("[data-capture-keys]"))
      return;
    const chord: Chord | null = chordFromEvent(e, platform);
    if (!chord) return;
    // Plain keys belong to whatever the person is typing in.
    if (!chord.mod && !chord.alt && typingIn(e.target)) return;
    const hit = parsed.find((b) => sameChord(b.chord, chord));
    const command = hit
      ? ctx.registry.commands.find((c) => c.id === hit.command)
      : undefined;
    if (!command) return;
    e.preventDefault();
    e.stopPropagation();
    void runAction(command.action, ctx);
  };
}
