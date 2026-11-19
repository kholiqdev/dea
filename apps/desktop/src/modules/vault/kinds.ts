import type { SecretKind } from "$lib/ipc/bindings/SecretKind";

/** The name of each type in the table and the inspector. */
export const KIND_LABELS: Record<SecretKind, string> = {
  token: "Token",
  password: "Password",
  sshKey: "SSH key",
  apiKey: "API key",
  oauthToken: "OAuth token",
};

/** The filter tabs, in the design's order. */
export const KIND_FILTERS: [SecretKind | "all", string][] = [
  ["all", "All types"],
  ["token", "Tokens"],
  ["password", "Passwords"],
  ["sshKey", "SSH keys"],
  ["apiKey", "API keys"],
  ["oauthToken", "OAuth tokens"],
];
