import { expect, test } from "vitest";
import {
  chordFromEvent,
  chordToString,
  formatChord,
  parseChord,
  sameChord,
} from "./chord";

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

test("parses modifiers in any order and case", () => {
  expect(parseChord("Mod+Shift+T")).toEqual({
    mod: true,
    shift: true,
    alt: false,
    key: "t",
  });
  expect(parseChord("shift+mod+t")).toEqual(parseChord("Mod+Shift+T"));
  expect(parseChord("Mod+,")).toEqual({
    mod: true,
    shift: false,
    alt: false,
    key: ",",
  });
  expect(parseChord("Mod++").key).toBe("+");
  expect(parseChord("alt+enter").key).toBe("Enter");
  expect(parseChord("Escape").mod).toBe(false);
});

test("rejects text that is not a chord", () => {
  for (const bad of ["", "Mod+", "Ctrl+T", "Mod+Mod+T", "Mod+Banana", "Shift"])
    expect(() => parseChord(bad), bad).toThrow();
});

test("writes chords back in one canonical form", () => {
  expect(chordToString(parseChord("shift+mod+t"))).toBe("Mod+Shift+T");
  expect(chordToString(parseChord("alt+mod+shift+1"))).toBe("Mod+Shift+Alt+1");
  expect(chordToString(parseChord("mod+escape"))).toBe("Mod+Escape");
});

test("digits_match_by_physical_key_on_azerty", () => {
  // AZERTY: Ctrl + the "1" key gives key "&" and code "Digit1"
  const e = key({ key: "&", code: "Digit1", ctrlKey: true });
  expect(chordFromEvent(e, "other")).toEqual(parseChord("Mod+1"));
});

test("letters_match_by_the_typed_character", () => {
  // a layout where the typed "t" sits on a different physical key
  const e = key({ key: "T", code: "KeyY", metaKey: true, shiftKey: true });
  expect(chordFromEvent(e, "mac")).toEqual(parseChord("Mod+Shift+T"));
});

test("letters fall back to the physical key when the layout types another script", () => {
  // Russian layout: the "T" key types "е"
  const e = key({ key: "е", code: "KeyT", ctrlKey: true });
  expect(chordFromEvent(e, "other")).toEqual(parseChord("Mod+T"));
  // macOS Option turns letters into symbols
  const alt = key({ key: "†", code: "KeyT", metaKey: true, altKey: true });
  expect(chordFromEvent(alt, "mac")).toEqual(parseChord("Mod+Alt+T"));
});

test("formats per platform", () => {
  expect(formatChord(parseChord("Mod+Shift+T"), "mac")).toBe("⌘⇧T");
  expect(formatChord(parseChord("Mod+Shift+T"), "other")).toBe("Ctrl+Shift+T");
  expect(formatChord(parseChord("Mod+,"), "mac")).toBe("⌘,");
  expect(formatChord(parseChord("Alt+Enter"), "other")).toBe("Alt+Enter");
});

test("modifier-only events are not chords", () => {
  for (const k of ["Shift", "Control", "Alt", "Meta"])
    expect(chordFromEvent(key({ key: k, ctrlKey: true }), "other")).toBeNull();
});

test("the other platform's command key is not Mod", () => {
  // Ctrl+1 on a Mac, and the Windows key on Windows, must not act as Mod+1
  expect(
    chordFromEvent(key({ key: "1", code: "Digit1", ctrlKey: true }), "mac"),
  ).toBeNull();
  expect(
    chordFromEvent(key({ key: "1", code: "Digit1", metaKey: true }), "other"),
  ).toBeNull();
});

test("named keys and punctuation", () => {
  expect(
    chordFromEvent(key({ key: ",", code: "Comma", metaKey: true }), "mac"),
  ).toEqual(parseChord("Mod+,"));
  expect(
    chordFromEvent(key({ key: "Escape", code: "Escape" }), "other"),
  ).toEqual(parseChord("Escape"));
  expect(
    chordFromEvent(key({ key: " ", code: "Space", ctrlKey: true }), "other"),
  ).toEqual(parseChord("Mod+Space"));
});

test("same chord compares every field", () => {
  expect(sameChord(parseChord("Mod+T"), parseChord("mod+t"))).toBe(true);
  expect(sameChord(parseChord("Mod+T"), parseChord("Mod+Shift+T"))).toBe(false);
});
