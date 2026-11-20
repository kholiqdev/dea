import { render, screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import ToastStack from "./ToastStack.svelte";
import { Toasts, toasts as appToasts } from "./toasts.svelte";

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

test("an action toast counts down the seconds left", () => {
  vi.useFakeTimers();
  const toasts = new Toasts();
  toasts.show("Deleted", {
    action: { label: "Undo", run: () => {} },
    timeout: 10_000,
  });
  expect(toasts.items[0]?.seconds).toBe(10);
  vi.advanceTimersByTime(1000);
  expect(toasts.items[0]?.seconds).toBe(9);
  vi.advanceTimersByTime(9000);
  expect(toasts.items).toEqual([]);
});

test("a toast without an action shows no countdown", () => {
  vi.useFakeTimers();
  const toasts = new Toasts();
  toasts.show("Saved");
  expect(toasts.items[0]?.seconds).toBeUndefined();
});

test("the stack shows the countdown and one keycap per key", () => {
  vi.useFakeTimers();
  render(ToastStack);
  const id = appToasts.show("Deleted", {
    action: { label: "Undo", run: () => {}, chord: "Mod+Z" },
    timeout: 10_000,
  });
  flushSync();
  expect(screen.getByText("10 s")).toBeInTheDocument();
  const keys = [...document.querySelectorAll("kbd")].map((k) => k.textContent);
  expect(keys).toHaveLength(2);
  expect(keys[1]).toBe("Z");
  vi.advanceTimersByTime(1000);
  flushSync();
  expect(screen.getByText("9 s")).toBeInTheDocument();
  appToasts.dismiss(id);
});

test("the countdown is not read out every second", () => {
  vi.useFakeTimers();
  render(ToastStack);
  const id = appToasts.show("Deleted", {
    action: { label: "Undo", run: () => {} },
    timeout: 10_000,
  });
  flushSync();
  expect(screen.getByText("10 s")).toHaveAttribute("aria-hidden", "true");
  appToasts.dismiss(id);
});
