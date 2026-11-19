// The Vault of the app: its status, and the password prompts it raises.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { ProjectInfo } from "./bindings/ProjectInfo";
import type { SecretInput } from "./bindings/SecretInput";
import type { SecretRow } from "./bindings/SecretRow";
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

/** The window's project secrets in every environment; works while locked. */
export function vaultSecrets(): Promise<SecretRow[]> {
  return invoke<SecretRow[]>("vault_secrets");
}

/** The value of a secret. Production asks the user first. */
export function vaultReveal(
  environment: string,
  name: string,
): Promise<string> {
  return invoke<string>("vault_reveal", { environment, name });
}

/** Copies the value; the clipboard clears after 30 s. */
export function vaultCopy(environment: string, name: string): Promise<void> {
  return invoke<void>("vault_copy", { environment, name });
}

/** Removes the value from this device; the declaration stays. */
export function vaultDeleteValue(
  environment: string,
  name: string,
): Promise<void> {
  return invoke<void>("vault_delete_value", { environment, name });
}

/**
 * Declares the secret in the project and stores its value. Returns the
 * project as it is now. The value passes here and nowhere else.
 */
export function vaultSaveSecret(input: SecretInput): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("vault_save_secret", { input });
}
