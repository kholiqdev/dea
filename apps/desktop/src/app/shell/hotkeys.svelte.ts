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
    case "openSettings":
      return ctx.shell.openModule("settings");
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
 * Listens for shortcuts on `window`, before anything else sees the key.
 * Returns a function that stops listening.
 */
export function installHotkeys(opts: {
  registry: Registry;
  shell: Shell;
  overrides: () => Record<string, string | null | undefined>;
  platform?: Platform;
}): () => void {
  const platform = opts.platform ?? currentPlatform();
  const parsed = $derived(
    effectiveShortcuts(opts.registry, opts.overrides()).flatMap((b) => {
      try {
        return [{ command: b.command, chord: parseChord(b.chord) }];
      } catch {
        return [];
      }
    }),
  );

  const onKey = (e: KeyboardEvent) => {
    // A shortcut recorder, such as the one in Settings, gets every key.
    if (e.target instanceof Element && e.target.closest("[data-capture-keys]"))
      return;
    const chord: Chord | null = chordFromEvent(e, platform);
    if (!chord) return;
    // Plain keys belong to whatever the person is typing in.
    if (!chord.mod && !chord.alt && typingIn(e.target)) return;
    const hit = parsed.find((b) => sameChord(b.chord, chord));
    const command = hit
      ? opts.registry.commands.find((c) => c.id === hit.command)
      : undefined;
    if (!command) return;
    e.preventDefault();
    e.stopPropagation();
    void runAction(command.action, opts);
  };

  window.addEventListener("keydown", onKey, { capture: true });
  return () => window.removeEventListener("keydown", onKey, { capture: true });
}
