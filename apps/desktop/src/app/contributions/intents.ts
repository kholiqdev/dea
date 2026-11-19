// Running an intent: one module asking another to do something without
// importing it (F1-FON-18).

import type { IntentContribution } from "./types";

export type IntentResult =
  { ok: true; value: unknown } | { ok: false; message: string };

export async function runIntent(
  intent: string,
  handler: IntentContribution | undefined,
  args: unknown,
): Promise<IntentResult> {
  if (!handler) return { ok: false, message: `No module handles "${intent}"` };
  try {
    const run = (await handler.handler()).default;
    return { ok: true, value: await run(args) };
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
}
