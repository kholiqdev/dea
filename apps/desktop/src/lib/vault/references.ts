// How environment values and secret names relate to the Vault.

/** Secret names a value refers to with `{{vault:name}}`. */
export function vaultReferences(value: string): string[] {
  return [...value.matchAll(/\{\{\s*vault:\s*([^}\s]+)\s*\}\}/g)].map(
    (m) => m[1] ?? "",
  );
}

/** Same rule as `is_valid_variable_name` in dea-project. */
export function isValidSecretName(name: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_.-]*$/.test(name);
}
