import { afterEach, expect, test, vi } from "vitest";
import { Toasts } from "./toasts.svelte";

afterEach(() => vi.useRealTimers());

test("a toast goes away after its timeout", () => {
  vi.useFakeTimers();
  const toasts = new Toasts();
  const id = toasts.show("Saved", { timeout: 1000 });
  expect(toasts.items.map((t) => t.id)).toEqual([id]);
  vi.advanceTimersByTime(1000);
  expect(toasts.items).toEqual([]);
});

test("an action keeps its chord", () => {
  vi.useFakeTimers();
  const toasts = new Toasts();
  toasts.show("Deleted", {
    action: { label: "Undo", run: () => {}, chord: "Mod+Z" },
  });
  expect(toasts.items[0]?.action?.chord).toBe("Mod+Z");
});
