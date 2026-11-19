// The Vault of the app: its status, and the password prompts it raises.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { UnlockMethod } from "./bindings/UnlockMethod";
import type { VaultStatus } from "./bindings/VaultStatus";

export interface VaultInfo {
  locked: boolean;
  method: UnlockMethod;
}

/** What went wrong with the last answer to a prompt. */
export type VaultPromptProblem =
  | { kind: "wrongPassword" }
  | { kind: "tooShort"; min: number }
  | { kind: "retryAfter"; seconds: number };

/** A question for the Vault password, sent to the window that asked. */
export interface VaultPromptEvent {
  id: number;
  kind: "password" | "newPassword";
  /** What Dea wants to do, as a lower case phrase. */
  reason: string;
  problem: VaultPromptProblem | null;
}

export function vaultInfo(): Promise<VaultInfo> {
  return invoke<VaultInfo>("vault_info");
}

export function vaultUnlock(): Promise<void> {
  return invoke<void>("vault_unlock");
}

export function vaultLock(): Promise<void> {
  return invoke<void>("vault_lock");
}

export function vaultActivity(): Promise<void> {
  return invoke<void>("vault_activity");
}

export function vaultAuthorize(action: string): Promise<void> {
  return invoke<void>("vault_authorize", { action });
}

/** Answers a prompt once; `null` cancels it. */
export function vaultAnswerPrompt(
  id: number,
  password: string | null,
): Promise<void> {
  return invoke<void>("vault_answer_prompt", { id, password });
}

/** Calls `handler` each time the Vault locks or unlocks. */
export function onVaultStatus(
  handler: (status: VaultStatus) => void,
): Promise<() => void> {
  return getCurrentWebviewWindow().listen<VaultStatus>("vault-status", (e) =>
    handler(e.payload),
  );
}

/**
 * Calls `handler` for each prompt of this window. It must listen on this
 * window: Tauri also delivers events sent to another window to a plain
 * `listen`.
 */
export function onVaultPrompt(
  handler: (prompt: VaultPromptEvent) => void,
): Promise<() => void> {
  return getCurrentWebviewWindow().listen<VaultPromptEvent>(
    "vault-prompt",
    (e) => handler(e.payload),
  );
}
