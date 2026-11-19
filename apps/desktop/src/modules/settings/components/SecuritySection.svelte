<!-- Settings > Security and privacy: when the Vault locks, and local data. -->
<script lang="ts">
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { methodPhrase, vaultApi } from "$app/contributions/vault-api";
  import { errorMessage } from "$lib/ipc/errors";
  import type { Security } from "$lib/ipc/bindings/Security";
  import { settings } from "$lib/settings/settings.svelte";

  const IDLE_MAX = 240;
  const PROD_MAX = 60;
  const DEFAULT_IDLE = 15;
  const security = $derived(settings.security);
  const hasProject = $derived(projectApi().info !== null);
  const never = $derived(security.autoLockMinutes === null);

  let idleError = $state<string | null>(null);
  let prodError = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function save(patch: Partial<Security>): Promise<void> {
    try {
      await settings.update({ security: patch });
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // A whole number from 1 to `max`; anything else (0, 241, 1.5, text,
  // nothing) is not saved and the field says so.
  function parse(text: string, max: number): number | null {
    if (!/^\d+$/.test(text.trim())) return null;
    const n = Number(text);
    return n >= 1 && n <= max ? n : null;
  }

  function commitIdle(input: HTMLInputElement): void {
    const n = parse(input.value, IDLE_MAX);
    if (n === null) {
      idleError = `Use a number from 1 to ${IDLE_MAX}`;
      return;
    }
    idleError = null;
    if (n !== security.autoLockMinutes) void save({ autoLockMinutes: n });
  }

  function commitProduction(input: HTMLInputElement): void {
    const n = parse(input.value, PROD_MAX);
    if (n === null) {
      prodError = `Use a number from 1 to ${PROD_MAX}`;
      return;
    }
    prodError = null;
    if (n !== security.productionMinutes) void save({ productionMinutes: n });
  }

  function toggleNever(checked: boolean): void {
    idleError = null;
    void save({
      autoLockMinutes: checked ? null : DEFAULT_IDLE,
    });
  }
</script>

<div class="page">
  <header class="page-head">
    <h1>Security and privacy</h1>
    <p class="meta">Dea keeps everything on this computer.</p>
  </header>

  <section class="group" aria-labelledby="vault-head">
    <h2 id="vault-head" class="group-title">Vault</h2>
    <div class="card">
      <div class="frow">
        <label class="lbl" for="idle">Lock when idle</label>
        <div class="field">
          <div class="inline">
            <input
              id="idle"
              class="input"
              type="number"
              inputmode="numeric"
              aria-label="Lock when idle, minutes"
              aria-invalid={idleError ? "true" : undefined}
              aria-describedby={idleError ? "idle-hint idle-err" : "idle-hint"}
              disabled={never}
              value={security.autoLockMinutes ?? ""}
              onkeydown={(e) =>
                e.key === "Enter" && commitIdle(e.currentTarget)}
              onchange={(e) => commitIdle(e.currentTarget)}
              onblur={(e) => commitIdle(e.currentTarget)}
            />
            <label class="check">
              <input
                type="checkbox"
                checked={never}
                onchange={(e) => toggleNever(e.currentTarget.checked)}
              />
              Never lock while Dea is open
            </label>
          </div>
          <span id="idle-hint" class="meta">From 1 to {IDLE_MAX}</span>
          {#if idleError}
            <span id="idle-err" class="error">{idleError}</span>
          {/if}
        </div>
      </div>
      <div class="frow">
        <label class="lbl" for="prod">Ask again for production</label>
        <div class="field">
          <input
            id="prod"
            class="input"
            type="number"
            inputmode="numeric"
            aria-label="Ask again for production, minutes"
            aria-invalid={prodError ? "true" : undefined}
            aria-describedby={prodError ? "prod-hint prod-err" : "prod-hint"}
            value={security.productionMinutes}
            onkeydown={(e) =>
              e.key === "Enter" && commitProduction(e.currentTarget)}
            onchange={(e) => commitProduction(e.currentTarget)}
            onblur={(e) => commitProduction(e.currentTarget)}
          />
          <span id="prod-hint" class="meta"
            >From 1 to {PROD_MAX}. After this, production secrets ask for {methodPhrase(
              vaultApi().method,
            )} again</span
          >
          {#if prodError}
            <span id="prod-err" class="error">{prodError}</span>
          {/if}
        </div>
      </div>
    </div>
  </section>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if hasProject}
    <section class="group" aria-labelledby="local-head">
      <h2 id="local-head" class="group-title">Local data</h2>
      <div class="card">
        <div class="frow">
          <span class="lbl">Clear local data</span>
          <div class="field">
            <span class="meta"
              >Removes history, cookies, saved examples and window state on this
              computer. Project files and Vault stay</span
            >
            <button
              type="button"
              class="btn"
              onclick={() => void shellApi().runCommand("shell.clearLocalData")}
            >
              Clear local data…
            </button>
          </div>
        </div>
      </div>
    </section>
  {/if}
</div>

<style>
  .page {
    max-width: 760px;
    padding: var(--s-8);
    display: flex;
    flex-direction: column;
    gap: var(--s-8);
  }

  .page-head h1 {
    font-size: 24px;
    line-height: 32px;
    font-weight: 500;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .group-title {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    padding: var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .frow {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    align-items: start;
    column-gap: var(--s-4);
  }

  .lbl {
    line-height: 36px;
    color: var(--text-2);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    align-items: flex-start;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: var(--s-4);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    color: var(--text-1);
  }

  .input {
    height: 36px;
    width: 96px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-input-strong);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .input:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .input[aria-invalid="true"] {
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .input:disabled {
    opacity: 0.5;
  }

  .btn {
    height: 32px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .btn:hover {
    background: var(--bg-hover);
  }

  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .error {
    font-size: 12px;
    line-height: 16px;
    color: var(--negative-text);
  }
</style>
