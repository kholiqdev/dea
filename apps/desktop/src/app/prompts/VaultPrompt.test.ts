import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushSync, tick } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { VaultPromptEvent } from "$lib/ipc/vault";
import { toasts } from "./toasts.svelte";
import VaultPrompt from "./VaultPrompt.svelte";
import { VaultPrompts } from "./vault-prompt.svelte";

let answers: { id: number; password: string | null }[];
let prompts: VaultPrompts;

beforeEach(() => {
  answers = [];
  prompts = new VaultPrompts();
  clearMocks();
  mockIPC((cmd, args) => {
    if (cmd === "vault_answer_prompt")
      answers.push(args as { id: number; password: string | null });
    return null;
  });
});

afterEach(() => {
  vi.useRealTimers();
});

function ask(over: Partial<VaultPromptEvent> = {}): void {
  prompts.show({
    id: 7,
    kind: "password",
    reason: "unlock the Vault",
    problem: null,
    ...over,
  });
  flushSync();
}

const field = (name: string) =>
  screen.getByLabelText<HTMLInputElement>(name, { selector: "input" });

test("a Vault password prompt sends the password and clears the field", async () => {
  render(VaultPrompt, { prompts });
  expect(screen.queryByRole("dialog")).toBeNull();
  ask();
  const dialog = screen.getByRole("dialog", { name: "Vault password" });
  expect(dialog).toHaveTextContent("Dea wants to unlock the Vault.");
  expect(field("Vault password")).toHaveFocus();
  await userEvent.type(field("Vault password"), "correct horse");
  await userEvent.click(within(dialog).getByRole("button", { name: "Unlock" }));
  expect(answers).toEqual([{ id: 7, password: "correct horse" }]);
  expect(prompts.current).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  // The next prompt starts with an empty field.
  ask({ id: 8 });
  expect(field("Vault password").value).toBe("");
});

test("Enter in the field unlocks", async () => {
  render(VaultPrompt, { prompts });
  ask();
  await userEvent.type(field("Vault password"), "correct horse{Enter}");
  expect(answers).toEqual([{ id: 7, password: "correct horse" }]);
});

test("Cancel answers null", async () => {
  render(VaultPrompt, { prompts });
  ask();
  await userEvent.type(field("Vault password"), "secret");
  await userEvent.click(screen.getByRole("button", { name: /Cancel/ }));
  expect(answers).toEqual([{ id: 7, password: null }]);
  ask({ id: 8 });
  await userEvent.keyboard("{Escape}");
  expect(answers).toEqual([
    { id: 7, password: null },
    { id: 8, password: null },
  ]);
});

test("a wrong password shows why and asks again", () => {
  render(VaultPrompt, { prompts });
  ask({ problem: { kind: "wrongPassword" } });
  expect(screen.getByRole("dialog")).toHaveTextContent(
    "The Vault password is wrong. Try again.",
  );
});

test("the wait after 5 wrong tries counts down before Unlock", async () => {
  vi.useFakeTimers();
  render(VaultPrompt, { prompts });
  ask({ problem: { kind: "retryAfter", seconds: 30 } });
  const dialog = screen.getByRole("dialog", {
    name: "Too many wrong passwords",
  });
  expect(dialog).toHaveTextContent(
    "You can try again in 30 seconds. This is the wait after 5 wrong tries.",
  );
  expect(screen.queryByLabelText("Vault password")).toBeNull();
  const unlock = within(dialog).getByRole("button", { name: "Unlock" });
  expect(unlock).toBeDisabled();

  vi.advanceTimersByTime(29_000);
  await tick();
  expect(dialog).toHaveTextContent("You can try again in 1 second.");
  expect(unlock).toBeDisabled();

  vi.advanceTimersByTime(1_000);
  await tick();
  expect(unlock).toBeEnabled();
  expect(answers).toEqual([]);
  unlock.click();
  await tick();
  expect(answers).toHaveLength(1);
  expect(answers[0]?.id).toBe(7);
  expect(prompts.current).toBeNull();
});

test("Cancel during the wait answers null", async () => {
  vi.useFakeTimers();
  render(VaultPrompt, { prompts });
  ask({ problem: { kind: "retryAfter", seconds: 30 } });
  screen.getByRole("button", { name: /Cancel/ }).click();
  await tick();
  expect(answers).toEqual([{ id: 7, password: null }]);
});

test("a new password must match and have 8 characters", async () => {
  render(VaultPrompt, { prompts });
  ask({ kind: "newPassword", reason: "reveal a value in production" });
  const dialog = screen.getByRole("dialog", {
    name: "Create a Vault password",
  });
  expect(dialog).toHaveTextContent(
    "This is the first production secret on this computer. Linux has no system prompt to protect it, so Dea asks for a password of your own.",
  );
  const create = within(dialog).getByRole("button", {
    name: "Create password",
  });

  await userEvent.type(field("New password"), "short");
  await userEvent.type(field("Repeat it"), "short");
  await userEvent.click(create);
  expect(dialog).toHaveTextContent("Use at least 8 characters");
  expect(answers).toEqual([]);

  await userEvent.clear(field("New password"));
  await userEvent.clear(field("Repeat it"));
  await userEvent.type(field("New password"), "long enough 1");
  await userEvent.type(field("Repeat it"), "long enough 2");
  await userEvent.click(create);
  expect(dialog).toHaveTextContent("The passwords do not match");
  expect(answers).toEqual([]);

  await userEvent.clear(field("Repeat it"));
  await userEvent.type(field("Repeat it"), "long enough 1");
  await userEvent.click(create);
  expect(answers).toEqual([{ id: 7, password: "long enough 1" }]);
});

test("the engine's too-short problem is shown", () => {
  render(VaultPrompt, { prompts });
  ask({ kind: "newPassword", problem: { kind: "tooShort", min: 8 } });
  expect(screen.getByRole("dialog")).toHaveTextContent(
    "Use at least 8 characters",
  );
});

test("a prompt replaced before it was answered is cancelled", async () => {
  ask();
  ask({ id: 9 });
  await expect.poll(() => answers).toEqual([{ id: 7, password: null }]);
  expect(prompts.current?.id).toBe(9);
});

test("a failed answer is shown, not swallowed", async () => {
  clearMocks();
  mockIPC(() => {
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- commands reject with { kind, message }
    throw { kind: "internal", message: "channel closed" };
  });
  ask();
  prompts.answer(null);
  await expect
    .poll(() => toasts.items.map((t) => t.text))
    .toContain("Channel closed");
  for (const t of toasts.items) toasts.dismiss(t.id);
});

test("once it listens, a page asks again for prompts it missed", async () => {
  const steps: string[] = [];
  let send: (p: VaultPromptEvent) => void = () => {};
  await prompts.listen(
    (handler) => {
      steps.push("subscribe");
      send = handler;
      return Promise.resolve(() => {});
    },
    () => {
      steps.push("resend");
      // The prompt that was sent while the page loaded comes again.
      send({
        id: 7,
        kind: "password",
        reason: "unlock the Vault",
        problem: null,
      });
      return Promise.resolve();
    },
  );
  expect(steps).toEqual(["subscribe", "resend"]);
  expect(prompts.current?.id).toBe(7);
});
