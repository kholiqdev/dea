// The Vault (F1-FON-07 to F1-FON-10). The cases run in order and share one
// app: each starts where the one before stopped. Linux has no system
// authenticator, so there every unlock and production check goes through
// Dea's own Vault password dialog, which WebDriver can answer. Windows
// asks through a native dialog, so only the status bar case runs there.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  MOD,
  answerVaultPassword,
  openProjectAt,
  textOf,
  vaultStatus,
  waitForText,
  writeProject,
} from "./helpers.js";

const PASSWORD = "correct horse battery";
const SECRETS = 'table[aria-label="Secrets"]';
const INSPECTOR = 'aside[aria-label="Secret"]';
const PROMPT_FORM = 'form[id^="vault-prompt-"]';
const STATUS = "footer .vault-status";

const work = mkdtempSync(join(tmpdir(), "dea-e2e-vault-"));

/** The id of the password question on screen; a new question has a new id. */
function promptId(): Promise<string | null> {
  return browser.execute(
    (s) => document.querySelector(s)?.id ?? null,
    PROMPT_FORM,
  );
}

/** Unlocks from the locked screen with the password made earlier. */
async function unlockWithPassword(): Promise<void> {
  await $("button*=Unlock with").click();
  await answerVaultPassword(PASSWORD);
  await waitForText(STATUS, "Vault unlocked", 30_000);
}

/** Fills the `Add a secret` page for `name` and saves it. */
async function addSecret(name: string, value: string): Promise<void> {
  await $("button=Add secret").click();
  const form = $("form.page");
  await form.waitForDisplayed();
  await form.$('input[id$="-name"]').setValue(name);
  await form.$('input[id$="-value"]').setValue(value);
  await form.$("button=Save secret").click();
  await waitForText(SECRETS, name);
}

before(async () => {
  await openProjectAt(writeProject("vault-api", work));
});
after(() => rmSync(work, { recursive: true, force: true }));

describe("the Vault in the status bar", () => {
  it("shows the Vault locked before anything unlocked it", async () => {
    await waitForText(STATUS, "Vault locked");
    expect(await vaultStatus()).toBe("Vault locked");
  });
});

(process.platform === "win32" ? describe.skip : describe)("the Vault", () => {
  it("creates a Vault password, adds a secret and lists it", async () => {
    await $(STATUS).click();
    await $("h2=Vault is locked").waitForDisplayed();

    await $("button*=Unlock with").click();
    // The first unlock makes the password.
    await waitForText('[role="dialog"]', "Create a Vault password");
    await answerVaultPassword(PASSWORD);
    await waitForText(STATUS, "Vault unlocked", 30_000);

    await addSecret("API_TOKEN", "local-secret-value");
    expect(await textOf(SECRETS)).toContain("API_TOKEN");
  });

  it("reveals a value and hides it again after 30 s", async () => {
    await $(SECRETS).$("button=API_TOKEN").click();
    await $(INSPECTOR).$("button=Reveal value").click();
    await waitForText(`${INSPECTOR} code`, "local-secret-value");

    await browser.pause(31_000); // F1-FON-08 #8
    await expect($(`${INSPECTOR} code`)).not.toBeExisting();
    await expect($(INSPECTOR).$("button=Reveal value")).toBeDisplayed();
  });

  it("locks with Lock now and the status changes within a second", async () => {
    await $("button=Lock now").click();
    await browser.waitUntil(
      async () => (await vaultStatus()) === "Vault locked",
      {
        timeout: 1_000,
        timeoutMsg: "the status bar still showed the Vault unlocked",
      },
    );
    await $("h2=Vault is locked").waitForDisplayed();
  });

  it("asks for the Vault password to reveal a production secret", async () => {
    await unlockWithPassword();
    await $('aside[aria-label="Vault sidebar"]')
      .$("button*=production")
      .click();
    await addSecret("PROD_TOKEN", "production-secret-value");

    // Unlocking does not open production: revealing asks again.
    await $(SECRETS).$("button=PROD_TOKEN").click();
    await $(INSPECTOR).$("button=Reveal value").click();
    await waitForText('[role="dialog"]', "production");
    expect(await textOf('[role="dialog"]')).toContain("Vault password");
    await answerVaultPassword(PASSWORD);
    await waitForText(`${INSPECTOR} code`, "production-secret-value", 30_000);
  });

  it("asks again before turning production off", async () => {
    // Locking closes the production window, so the switch has to ask.
    await $("button=Lock now").click();
    await waitForText(STATUS, "Vault locked");

    await browser.keys([MOD, "e"]);
    await $('[role="menu"]').$("button*=Edit environments").click();
    await $("main")
      .$("nav[aria-label='Environments']")
      .$("button*=production")
      .click();
    const production = $('button[aria-label="Treat as production"]');
    await expect(production).toHaveAttribute("aria-checked", "true");

    // Cancelling the question leaves production on.
    await production.click();
    await waitForText('[role="dialog"]', "turn off production");
    await $('[role="dialog"]').$("button*=Cancel").click();
    await $('[role="dialog"]').waitForExist({ reverse: true });
    await expect(production).toHaveAttribute("aria-checked", "true");

    // Answering it turns production off.
    await production.click();
    await waitForText('[role="dialog"]', "turn off production");
    await answerVaultPassword(PASSWORD);
    await browser.waitUntil(
      async () => (await production.getAttribute("aria-checked")) === "false",
      { timeout: 30_000, timeoutMsg: "production was still on" },
    );
  });

  it("five wrong passwords hold the next try for 30 s", async () => {
    // The answer above unlocked the Vault: lock it first.
    await $(STATUS).click();
    await $("button=Lock now").click();
    await $("h2=Vault is locked").waitForDisplayed();

    await $("button*=Unlock with").click();
    for (let i = 0; i < 5; i++) {
      await $(PROMPT_FORM).waitForExist({ timeout: 30_000 });
      const before = await promptId();
      await answerVaultPassword("not the password");
      // The dialog asks again: a new question has a new id.
      await browser.waitUntil(
        async () => {
          const now = await promptId();
          return now !== null && now !== before;
        },
        {
          timeout: 30_000,
          timeoutMsg: `no new question after wrong try ${i + 1}`,
        },
      );
    }
    await waitForText('[role="dialog"]', "Too many wrong passwords");
    expect(await textOf('[role="dialog"]')).toContain("You can try again in");

    // The Vault stays locked.
    await $('[role="dialog"]').$("button*=Cancel").click();
    await $('[role="dialog"]').waitForExist({ reverse: true });
    expect(await vaultStatus()).toBe("Vault locked");
  });
});
