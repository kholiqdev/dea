// Start-up order: a new window listens before it reads what it listens to.

import { mockWindows } from "@tauri-apps/api/mocks";
import { expect, test } from "vitest";
import { fakeIpc } from "./shell/test-support";
import { start } from "./start";

test("a new window listens first, then reads and asks for missed prompts", async () => {
  const log = fakeIpc();
  // The window APIs need a current window.
  mockWindows("main");
  const target = document.body.appendChild(document.createElement("div"));
  await start(target);

  const at = (cmd: string, event?: string): number =>
    log.calls.findIndex(
      (c) =>
        c.cmd === cmd &&
        (event === undefined ||
          (c.args as { event?: string } | undefined)?.event === event),
    );
  const statusListener = at("plugin:event|listen", "vault-status");
  const promptListener = at("plugin:event|listen", "vault-prompt");
  const settingsListener = at("plugin:event|listen", "settings-changed");
  for (const listener of [statusListener, promptListener, settingsListener])
    expect(listener).toBeGreaterThanOrEqual(0);

  // A status change or saved settings that come while the window reads
  // them are not missed.
  expect(statusListener).toBeLessThan(at("vault_info"));
  expect(settingsListener).toBeLessThan(at("get_settings"));
  // A prompt sent while the page loaded comes again once it listens.
  await expect
    .poll(() => at("vault_resend_prompts"))
    .toBeGreaterThan(promptListener);
});
