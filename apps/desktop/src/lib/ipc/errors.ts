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

/** A message to show for anything a command rejected with. */
export function errorMessage(e: unknown): string {
  if (isIpcError(e)) return e.message;
  return e instanceof Error ? e.message : String(e);
}
