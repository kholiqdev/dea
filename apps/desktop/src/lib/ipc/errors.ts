// The error shape every Dea command rejects with.

export interface IpcError {
  kind: string;
  message: string;
}

export function isIpcError(e: unknown): e is IpcError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as IpcError).kind === "string" &&
    typeof (e as IpcError).message === "string"
  );
}

/**
 * A message to show for anything a command rejected with. Dea's own
 * messages are lower case, as Rust errors are; shown alone they start with
 * a capital.
 */
export function errorMessage(e: unknown): string {
  const text = isIpcError(e)
    ? e.message
    : e instanceof Error
      ? e.message
      : String(e);
  return text.charAt(0).toUpperCase() + text.slice(1);
}
