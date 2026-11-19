import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { VaultStatus } from "$lib/ipc/bindings/VaultStatus";
import { toasts } from "../prompts/toasts.svelte";
import { methodPhrase, unlockLabel } from "../contributions/vault-api";
import { VaultPrompts } from "../prompts/vault-prompt.svelte";
import { WindowVault } from "./vault.svelte";

let calls: { cmd: string; args: unknown }[];
let failWith: unknown = null;

beforeEach(() => {
  calls = [];
  failWith = null;
  clearMocks();
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "vault_info") return { locked: true, method: "deviceOwner" };
    if (failWith && (cmd === "vault_unlock" || cmd === "vault_authorize"))
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- commands reject with { kind, message }
      throw failWith;
    return null;
  });
});

afterEach(() => {
  vi.useRealTimers();
  for (const t of toasts.items) toasts.dismiss(t.id);
});

test("load reads the status and the way this device unlocks", async () => {
  const v = new WindowVault();
  expect(v.locked).toBe(true);
  await v.load();
  expect(v.method).toBe("deviceOwner");
  expect(v.locked).toBe(true);
});

test("every window follows the Vault status", async () => {
  const v = new WindowVault();
  let send: (s: VaultStatus) => void = () => {};
  await v.listen((handler) => {
    send = handler;
    return Promise.resolve(() => {});
  });
  let seen = 0;
  const stop = $effect.root(() => {
    $effect(() => {
      void v.locked;
      seen++;
    });
  });
  flushSync();
  const before = seen;
  send({ locked: false });
  flushSync();
  expect(v.locked).toBe(false);
  expect(seen - before).toBe(1);
  send({ locked: true });
  expect(v.locked).toBe(true);
  stop();
});

test("activity is sent at most every 10 s, and only while unlocked", () => {
  vi.useFakeTimers();
  const v = new WindowVault();
  const target = new EventTarget();
  const stop = v.trackActivity(target);
  const activity = () => calls.filter((c) => c.cmd === "vault_activity").length;

  target.dispatchEvent(new Event("keydown"));
  expect(activity()).toBe(0); // locked

  v.apply({ locked: false });
  target.dispatchEvent(new Event("keydown"));
  target.dispatchEvent(new Event("pointerdown"));
  expect(activity()).toBe(1);

  vi.advanceTimersByTime(9_999);
  target.dispatchEvent(new Event("keydown"));
  expect(activity()).toBe(1);

  vi.advanceTimersByTime(1);
  target.dispatchEvent(new Event("pointerdown"));
  expect(activity()).toBe(2);

  stop();
  vi.advanceTimersByTime(20_000);
  target.dispatchEvent(new Event("keydown"));
  expect(activity()).toBe(2);
});

test("unlock returns false when cancelled and shows other errors", async () => {
  const v = new WindowVault();
  expect(await v.unlock()).toBe(true);

  failWith = { kind: "cancelled", message: "cancelled" };
  expect(await v.unlock()).toBe(false);
  expect(await v.authorize("turn off production for staging")).toBe(false);
  expect(toasts.items).toEqual([]);

  failWith = { kind: "authFailed", message: "authentication failed" };
  expect(await v.unlock()).toBe(false);
  expect(toasts.items.map((t) => t.text)).toEqual(["Authentication failed"]);
  expect(await v.authorize("x")).toBe(false);
  expect(toasts.items).toHaveLength(2);
});

test("authorize sends the action and resolves true", async () => {
  const v = new WindowVault();
  expect(await v.authorize("turn off production for staging")).toBe(true);
  expect(calls).toContainEqual({
    cmd: "vault_authorize",
    args: { action: "turn off production for staging" },
  });
});

test("lock asks the Vault to lock", async () => {
  const v = new WindowVault();
  await v.lock();
  expect(calls.map((c) => c.cmd)).toContain("vault_lock");
});

test("the unlock label and phrase follow the way to unlock", () => {
  expect(unlockLabel("deviceOwner")).toBe("Unlock with Touch ID");
  expect(unlockLabel("windowsHello")).toBe("Unlock with Windows Hello");
  expect(unlockLabel("windowsPassword")).toBe(
    "Unlock with your Windows password",
  );
  expect(unlockLabel("vaultPassword")).toBe("Unlock with your Vault password");
  expect(methodPhrase("deviceOwner")).toBe("Touch ID");
  expect(methodPhrase("windowsHello")).toBe("Windows Hello");
  expect(methodPhrase("windowsPassword")).toBe("your Windows password");
  expect(methodPhrase("vaultPassword")).toBe("your Vault password");
});

const wait = {
  id: 1,
  kind: "password" as const,
  reason: "unlock the Vault",
  problem: { kind: "retryAfter" as const, seconds: 30 },
};
const tooMany = { kind: "retryAfter", message: "try again in 30 s" };

function unlockAfter(
  prompts: VaultPrompts,
  how: "wait" | "cancel",
  then: "ok" | "again",
): WindowVault {
  let n = 0;
  clearMocks();
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd !== "vault_unlock") return null;
    n++;
    if (n > 1) {
      if (then === "ok") return null;
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- commands reject with { kind, message }
      throw tooMany;
    }
    prompts.show(wait);
    prompts.answer(how === "wait" ? "" : null);
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- commands reject with { kind, message }
    throw tooMany;
  });
  return new WindowVault(prompts);
}

test("a wait that was sat out retries the unlock once, without a toast", async () => {
  const v = unlockAfter(new VaultPrompts(), "wait", "ok");
  expect(await v.unlock()).toBe(true);
  expect(calls.filter((c) => c.cmd === "vault_unlock")).toHaveLength(2);
  expect(toasts.items).toEqual([]);
});

test("the retry happens once; a second wait is shown", async () => {
  const v = unlockAfter(new VaultPrompts(), "wait", "again");
  expect(await v.unlock()).toBe(false);
  expect(calls.filter((c) => c.cmd === "vault_unlock")).toHaveLength(2);
  expect(toasts.items).toHaveLength(1);
});

test("cancelling the wait is silent", async () => {
  const v = unlockAfter(new VaultPrompts(), "cancel", "ok");
  expect(await v.unlock()).toBe(false);
  expect(calls.filter((c) => c.cmd === "vault_unlock")).toHaveLength(1);
  expect(toasts.items).toEqual([]);
});
