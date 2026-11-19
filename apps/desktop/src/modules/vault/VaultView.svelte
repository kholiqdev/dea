<!-- The Vault module: the locked screen, or the secrets once unlocked. -->
<script lang="ts">
  import { untrack } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { vaultApi } from "$app/contributions/vault-api";
  import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
  import type { SecretKind } from "$lib/ipc/bindings/SecretKind";
  import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
  import { errorMessage } from "$lib/ipc/errors";
  import { settings } from "$lib/settings/settings.svelte";
  import { vaultDeleteValue } from "./api";
  import AddSecret from "./components/AddSecret.svelte";
  import DeleteValue from "./components/DeleteValue.svelte";
  import LockedVault from "./components/LockedVault.svelte";
  import SecretInspector from "./components/SecretInspector.svelte";
  import SecretsTable from "./components/SecretsTable.svelte";
  import VaultFooter from "./components/VaultFooter.svelte";
  import { KIND_FILTERS } from "./kinds";
  import { addSecret } from "./state/add-secret.svelte";
  import { secrets } from "./state/secrets.svelte";

  const vault = vaultApi();
  const project = projectApi();
  const info = $derived(project.info);
  const environment = $derived(info ? secrets.current(info) : null);
  const inEnvironment = $derived(environment ? secrets.of(environment) : []);
  const missing = $derived(inEnvironment.filter((r) => r.missing).length);
  let filter = $state<SecretKind | "all">("all");
  const shown = $derived(
    inEnvironment.filter((r) => filter === "all" || r.kind === filter),
  );
  // The choice is the row's place, so a reload keeps it and another
  // environment starts with none. The palette sets it too.
  const selected = $derived(
    inEnvironment.find(
      (r) =>
        r.environment === secrets.choice?.environment &&
        r.name === secrets.choice.name,
    ) ?? null,
  );
  let deleting = $state<SecretRow | null>(null);

  const summary = $derived.by(() => {
    const text = `${inEnvironment.length === 1 ? "1 secret" : `${inEnvironment.length} secrets`} in ${info?.name}`;
    if (missing === 0) return text;
    return missing === 1
      ? `${text}, 1 is missing a value`
      : `${text}, ${missing} are missing a value`;
  });

  $effect(() => {
    void info;
    void vault.locked;
    untrack(() => {
      secrets.load().catch((e: unknown) => shellApi().toast(errorMessage(e)));
    });
  });

  function pick(row: SecretRow): void {
    secrets.choice = { environment: row.environment, name: row.name };
  }

  function startAdd(row?: SecretRow): void {
    if (!environment) return;
    addSecret.open(
      row
        ? {
            environment: row.environment,
            name: row.name,
            kind: row.kind,
            description: row.description ?? "",
            locked: true,
          }
        : {
            environment,
            name: "",
            kind: "token",
            description: "",
            locked: false,
          },
    );
  }

  function saved(updated: ProjectInfo, env: string, name: string): void {
    addSecret.close();
    project.refresh(updated);
    secrets.chosen = env;
    secrets.choice = { environment: env, name };
    secrets.load().catch((e: unknown) => shellApi().toast(errorMessage(e)));
  }

  async function confirmDelete(): Promise<void> {
    const target = deleting;
    deleting = null;
    if (!target) return;
    try {
      await vaultDeleteValue(target.environment, target.name);
      await secrets.load();
    } catch (e) {
      shellApi().toast(errorMessage(e));
    }
  }
</script>

<div class="vault">
  <div class="body">
    {#if addSecret.draft && info}
      <!-- Kept while the Vault is locked, so a lock does not lose the form. -->
      <AddSecret
        {info}
        draft={addSecret.draft}
        onsaved={saved}
        oncancel={() => addSecret.close()}
      />
    {:else if vault.locked}
      <LockedVault />
    {:else if info && environment}
      <div class="main">
        <div class="toolbar">
          <div class="heading">
            <h1>{environment}</h1>
            <span class="meta">{summary}</span>
          </div>
          <div class="end">
            <Button variant="ghost" onclick={() => vault.lock()}
              >Lock now</Button
            >
            <Button variant="primary" onclick={() => startAdd()}
              >Add secret</Button
            >
          </div>
        </div>
        <div class="tabs" role="tablist" aria-label="Type">
          {#each KIND_FILTERS as [value, label] (value)}
            <button
              type="button"
              role="tab"
              class="tab"
              aria-selected={filter === value}
              onclick={() => (filter = value)}>{label}</button
            >
          {/each}
        </div>
        <div class="work">
          <section class="pane" aria-label="Secrets in {environment}">
            <SecretsTable
              rows={shown}
              {info}
              {selected}
              onselect={pick}
              onadd={startAdd}
            />
          </section>
          {#if selected}
            {#key `${selected.environment}/${selected.name}`}
              <SecretInspector
                row={selected}
                {info}
                ondelete={() => (deleting = selected)}
              />
            {/key}
          {/if}
        </div>
      </div>
    {:else}
      <p class="empty">Open a project to see its secrets.</p>
    {/if}
  </div>
  <VaultFooter autoLockMinutes={settings.value.security.autoLockMinutes} />
</div>

{#if deleting}
  <DeleteValue
    name={deleting.name}
    environment={deleting.environment}
    oncancel={() => (deleting = null)}
    ondelete={confirmDelete}
  />
{/if}

<style>
  .vault {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .heading {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  h1 {
    font-size: 15px;
    line-height: 20px;
    font-weight: 500;
  }

  .meta {
    font-size: 12px;
    color: var(--text-3);
  }

  .end {
    margin-left: auto;
    display: flex;
    gap: var(--s-2);
  }

  .tabs {
    display: flex;
    gap: var(--s-1);
    padding: var(--s-2) var(--s-4);
  }

  .tab {
    height: 28px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--text-2);
  }

  .tab:hover {
    background: var(--bg-hover);
  }

  .tab[aria-selected="true"] {
    background: var(--bg-3);
    color: var(--text-1);
  }

  .work {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .pane {
    flex: 1;
    min-width: 0;
    overflow: auto;
  }

  .empty {
    margin: var(--s-4);
    color: var(--text-3);
  }
</style>
