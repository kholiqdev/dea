// Keyboard chords such as `Mod+Shift+T`. `Mod` is Cmd on macOS and Ctrl
// elsewhere. Digits match the physical key, so `Mod+1` works on AZERTY;
// letters match the typed character, so `Mod+T` follows the layout.

export type Platform = "mac" | "other";

export interface Chord {
  mod: boolean;
  shift: boolean;
  alt: boolean;
  /** A lowercase letter, a digit, a punctuation mark or a key name. */
  key: string;
}

// Named keys, by lowercase name, in their canonical spelling.
const NAMED = new Map(
  [
    "Enter",
    "Escape",
    "Tab",
    "Space",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    ...Array.from({ length: 12 }, (_, i) => `F${i + 1}`),
  ].map((k) => [k.toLowerCase(), k]),
);

const MODIFIER_KEYS = new Set([
  "Shift",
  "Control",
  "Alt",
  "AltGraph",
  "Meta",
  "OS",
  "CapsLock",
]);

const MAC_KEYS: Record<string, string> = {
  Enter: "↩",
  Escape: "⎋",
  Tab: "⇥",
  Backspace: "⌫",
  Delete: "⌦",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

export function currentPlatform(): Platform {
  return /Mac|iPhone|iPad/.test(navigator.userAgent) ? "mac" : "other";
}

function normalizeKey(text: string): string | null {
  if (text.length === 1) return text === " " ? "Space" : text.toLowerCase();
  return NAMED.get(text.toLowerCase()) ?? null;
}

export function parseChord(text: string): Chord {
  // "Mod++" ends in the plus key itself.
  const parts = text.endsWith("++")
    ? [...text.slice(0, -2).split("+"), "+"]
    : text.split("+");
  const keyText = parts.pop() ?? "";
  const chord: Chord = { mod: false, shift: false, alt: false, key: "" };
  for (const part of parts) {
    const name = part.toLowerCase();
    if (name !== "mod" && name !== "shift" && name !== "alt")
      throw new Error(
        `"${text}" is not a shortcut: unknown modifier "${part}"`,
      );
    if (chord[name]) throw new Error(`"${text}" repeats "${part}"`);
    chord[name] = true;
  }
  const key = normalizeKey(keyText);
  if (!key || MODIFIER_KEYS.has(keyText))
    throw new Error(`"${text}" is not a shortcut: no key`);
  chord.key = key;
  return chord;
}

function keyLabel(key: string): string {
  return key.length === 1 ? key.toUpperCase() : key;
}

/** The canonical text of a chord, as stored in settings. */
export function chordToString(c: Chord): string {
  return [c.mod && "Mod", c.shift && "Shift", c.alt && "Alt", keyLabel(c.key)]
    .filter(Boolean)
    .join("+");
}

/** A chord as shown to people: `⌘⇧T` on macOS, `Ctrl+Shift+T` elsewhere. */
export function formatChord(c: Chord, p: Platform): string {
  if (p === "mac")
    return (
      (c.mod ? "⌘" : "") +
      (c.shift ? "⇧" : "") +
      (c.alt ? "⌥" : "") +
      (MAC_KEYS[c.key] ?? keyLabel(c.key))
    );
  return [c.mod && "Ctrl", c.shift && "Shift", c.alt && "Alt", keyLabel(c.key)]
    .filter(Boolean)
    .join("+");
}

function eventKey(e: KeyboardEvent): string | null {
  const digit = /^(?:Digit|Numpad)(\d)$/.exec(e.code)?.[1];
  if (digit) return digit;
  if (/^[a-z]$/i.test(e.key)) return e.key.toLowerCase();
  // Another script, or a symbol from macOS Option: use the physical letter.
  const letter = /^Key([A-Z])$/.exec(e.code)?.[1];
  if (letter) return letter.toLowerCase();
  return normalizeKey(e.key);
}

/** The chord a key press makes, or null for a lone modifier. */
export function chordFromEvent(e: KeyboardEvent, p: Platform): Chord | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  // The other platform's command key has no meaning in Dea's shortcuts.
  if (p === "mac" ? e.ctrlKey : e.metaKey) return null;
  const key = eventKey(e);
  if (!key) return null;
  return {
    mod: p === "mac" ? e.metaKey : e.ctrlKey,
    shift: e.shiftKey,
    alt: e.altKey,
    key,
  };
}

export function sameChord(a: Chord, b: Chord): boolean {
  return (
    a.mod === b.mod && a.shift === b.shift && a.alt === b.alt && a.key === b.key
  );
}
