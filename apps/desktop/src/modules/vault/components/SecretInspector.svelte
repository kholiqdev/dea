<!--
  One secret. A revealed value lives only in this component: it goes after
  30 s, when the Vault locks, and with the component, which another secret
  or leaving the module removes.
-->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import { shellApi } from "$app/contributions/shell-api";
  import { vaultApi } from "$app/contributions/vault-api";
  import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
  import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
  import { errorMessage, isIpcError } from "$lib/ipc/errors";
  import { vaultCopy, vaultReveal } from "../api";
  import { KIND_LABELS } from "../kinds";
  import { usedBy } from "../state/secrets.svelte";

  let {
    row,
    info,
    ondelete,
  }: { row: SecretRow; info: ProjectInfo; ondelete: () => void } = $props();

  /** How long a revealed value stays (F1-FON-08 #8). */
  const REVEAL_MS = 30_000;

  const vault = vaultApi();
  const users = $derived(usedBy(info, row.environment, row.name));
  let revealed = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;

  function hide(): void {
    clearTimeout(timer);
    revealed = null;
  }

  // The value goes with the component, and when the Vault locks.
  $effect(() => {
    alive = true;
    return () => {
      alive = false;
      hide();
    };
  });
  $effect(() => {
    if (vault.locked) hide();
  });

  /** A cancelled check is the user's answer, not a problem to report. */
  function fail(e: unknown): void {
    if (isIpcError(e) && e.kind === "cancelled") return;
    shellApi().toast(errorMessage(e));
  }

  async function reveal(): Promise<void> {
    try {
      const value = await vaultReveal(row.environment, row.name);
      if (!alive || vault.locked) return;
      revealed = value;
      clearTimeout(timer);
      timer = setTimeout(hide, REVEAL_MS);
    } catch (e) {
      fail(e);
    }
  }

  async function copy(): Promise<void> {
    try {
      await vaultCopy(row.environment, row.name);
      shellApi().toast("Copied. The clipboard clears in 30 s");
    } catch (e) {
      fail(e);
    }
  }
</script>

<aside class="inspector" aria-label="Secret">
  <div class="title">
    <h2>{row.name}</h2>
    <span class="meta"
      >{KIND_LABELS[row.kind]} in {info.name} / {row.environment}</span
    >
    {#if row.description}<span class="meta">{row.description}</span>{/if}
  </div>

  <div class="group">
    <h3 class="field-label">Value</h3>
    {#if row.missing}
      <span class="meta">No value on this device</span>
    {:else}
      <div class="value">
        {#if revealed !== null}<code>{revealed}</code>{:else}<span
            aria-hidden="true">••••••••••••</span
          >{/if}
      </div>
      <div class="actions">
        {#if revealed === null}
          <Button size="small" onclick={reveal}>Reveal value</Button>
        {:else}
          <Button size="small" onclick={hide}>Hide value</Button>
        {/if}
        <Button size="small" onclick={copy}>Copy value</Button>
      </div>
      <span class="meta">Copied values clear from the clipboard after 30 s</span
      >
    {/if}
  </div>

  <div class="group">
    <h3 class="field-label">Used by</h3>
    {#if users.length === 0}
      <span class="meta">—</span>
    {:else}
      <ul class="users">
        {#each users as user (user)}
          <li>
            <span class="mono">{user}</span>
            <span class="meta">Variable in {row.environment}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if !row.missing}
    <Button variant="danger" onclick={ondelete}>Delete value…</Button>
  {/if}
</aside>

<style>
  .inspector {
    width: 320px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    padding: var(--s-4);
    border-left: 1px solid var(--line);
    overflow: auto;
  }

  .title {
    display: flex;
    flex-direction: column;
  }

  h2 {
    font-size: 15px;
    line-height: 20px;
    font-weight: 500;
    font-family: var(--font-mono);
  }

  .field-label {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    align-items: flex-start;
  }

  .meta {
    font-size: 12px;
    color: var(--text-3);
  }

  .value {
    align-self: stretch;
    padding: var(--s-2) var(--s-3);
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 400 13px/20px var(--font-mono);
    color: var(--text-2);
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    gap: var(--s-2);
  }

  .users {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }

  .users li {
    display: flex;
    flex-direction: column;
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>
