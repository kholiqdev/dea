import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { expect, test, vi } from "vitest";
import FileBanner from "./FileBanner.svelte";

const message = createRawSnippet(() => ({
  render: () =>
    "<span><code>environments/staging.yaml</code> changed on disk while you were editing it.</span>",
}));

test("shows the message and runs the chosen action", async () => {
  const keep = vi.fn();
  const take = vi.fn();
  render(FileBanner, {
    tone: "warning",
    children: message,
    actions: [
      { label: "Keep mine", onclick: keep },
      { label: "Take theirs", onclick: take },
    ],
  });
  expect(screen.getByRole("status")).toHaveTextContent(
    "environments/staging.yaml changed on disk while you were editing it.",
  );
  await userEvent.click(screen.getByRole("button", { name: "Take theirs" }));
  expect(take).toHaveBeenCalledOnce();
  expect(keep).not.toHaveBeenCalled();
});

test("a banner without actions has no buttons", () => {
  render(FileBanner, { tone: "neutral", children: message });
  expect(screen.getByRole("status")).toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});

test("a ghost action is the quieter choice", () => {
  render(FileBanner, {
    tone: "warning",
    children: message,
    actions: [
      { label: "Save it again", onclick: () => {} },
      { label: "Discard changes", onclick: () => {}, variant: "ghost" },
    ],
  });
  expect(screen.getByRole("button", { name: "Discard changes" })).toHaveClass(
    "ghost",
  );
  expect(screen.getByRole("button", { name: "Save it again" })).toHaveClass(
    "secondary",
  );
});

test("a_banner_action_can_show_keys", () => {
  render(FileBanner, {
    tone: "warning",
    children: message,
    actions: [
      { label: "Send anyway", onclick: () => {}, keys: ["Ctrl", "Enter"] },
    ],
  });
  const button = screen.getByRole("button", { name: "Send anyway" });
  const keys = [...button.querySelectorAll("kbd.kc")];
  expect(keys.map((k) => k.textContent)).toEqual(["Ctrl", "Enter"]);
  expect(keys.every((k) => k.closest('[aria-hidden="true"]') !== null)).toBe(
    true,
  );
});
