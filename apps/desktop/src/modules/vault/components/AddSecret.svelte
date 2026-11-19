<!--
  The `Add a secret` page. The typed value lives only in this component: it
  goes on Cancel, on save and when the page goes. Reveal and Copy act on it
  here, without asking Dea.
-->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import { vaultApi } from "$app/contributions/vault-api";
  import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
  import type { SecretKind } from "$lib/ipc/bindings/SecretKind";
  import { errorMessage } from "$lib/ipc/errors";
  import { vaultSaveSecret } from "../api";
  import { KIND_LABELS } from "../kinds";
  import {
    clearClipboardLater,
    HIDE_MS,
    problems,
    toRfc3339,
    type Draft,
  } from "../state/add-secret.svelte";

  let {
    info,
    draft,
    onsaved,
    oncancel,
  }: {
    info: ProjectInfo;
    draft: Draft;
    onsaved: (info: ProjectInfo, environment: string, name: string) => void;
    oncancel: () => void;
  } = $props();

  const vault = vaultApi();
  const id = $props.id();

  // The form starts from the draft once; opening another remounts it.
  // svelte-ignore state_referenced_locally
  let name = $state(draft.name);
  // svelte-ignore state_referenced_locally
  let kind = $state<SecretKind>(draft.kind);
  // svelte-ignore state_referenced_locally
  let environment = $state(draft.environment);
  // svelte-ignore state_referenced_locally
  let description = $state(draft.description);
  let value = $state("");
  let refreshToken = $state("");
  let expires = $state("");
  let shown = $state(false);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let submitted = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const found = $derived(problems(name, value));
  const oauth = $derived(kind === "oauthToken");

  function mask(): void {
    clearTimeout(hideTimer);
    shown = false;
  }

  // The value goes with the page, and when the Vault locks.
  $effect(() => mask);
  $effect(() => {
    if (vault.locked) mask();
  });

  function reveal(): void {
    shown = true;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(mask, HIDE_MS);
  }

  async function copy(): Promise<void> {
    if (value === "") return;
    const text = value;
    try {
      await navigator.clipboard.writeText(text);
      clearClipboardLater(text);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function forget(): void {
    mask();
    value = "";
    refreshToken = "";
  }

  function cancel(): void {
    forget();
    oncancel();
  }

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    submitted = true;
    error = null;
    if (found.name || found.value || busy) return;
    busy = true;
    try {
      if (vault.locked && !(await vault.unlock())) return;
      const trimmed = name.trim();
      const updated = await vaultSaveSecret({
        name: trimmed,
        kind,
        environment,
        description: description.trim() === "" ? null : description.trim(),
        value,
        refreshToken: oauth && refreshToken !== "" ? refreshToken : null,
        expiresAt: oauth ? toRfc3339(expires) : null,
      });
      forget();
      onsaved(updated, environment, trimmed);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<form class="page" onsubmit={save} novalidate>
  <div class="head">
    <h1>Add a secret</h1>
    <p>
      Saved encrypted on this device, for the {environment} environment of {info.name}
    </p>
  </div>

  <div class="frow">
    <label class="lbl" for="{id}-name">Name</label>
    <input
      id="{id}-name"
      class="input mono"
      type="text"
      autocomplete="off"
      spellcheck="false"
      disabled={draft.locked}
      aria-invalid={submitted && found.name ? "true" : undefined}
      aria-describedby={submitted && found.name
        ? `${id}-name-problem`
        : undefined}
      bind:value={name}
    />
    {#if submitted && found.name}
      <span class="problem" id="{id}-name-problem">{found.name}</span>
    {/if}
  </div>

  <div class="frow">
    <label class="lbl" for="{id}-kind">Type</label>
    <select
      id="{id}-kind"
      class="input"
      disabled={draft.locked}
      bind:value={kind}
    >
      {#each Object.entries(KIND_LABELS) as [k, label] (k)}
        <option value={k}>{label}</option>
      {/each}
    </select>
  </div>

  <div class="frow">
    <label class="lbl" for="{id}-env">Environment</label>
    <select id="{id}-env" class="input" bind:value={environment}>
      {#each info.environments as env (env.name)}
        <option value={env.name}>{env.name}</option>
      {/each}
    </select>
  </div>

  <div class="frow">
    <label class="lbl" for="{id}-value">Value</label>
    <span class="pick">
      <input
        id="{id}-value"
        class="input mono"
        type={shown ? "text" : "password"}
        autocomplete="off"
        spellcheck="false"
        aria-invalid={submitted && found.value ? "true" : undefined}
        aria-describedby={submitted && found.value
          ? `${id}-value-problem`
          : undefined}
        bind:value
      />
      <Button size="small" onclick={shown ? mask : reveal}
        >{shown ? "Hide" : "Reveal"}</Button
      >
      <Button size="small" onclick={copy}>Copy</Button>
    </span>
    {#if submitted && found.value}
      <span class="problem" id="{id}-value-problem">{found.value}</span>
    {/if}
    <span class="meta"
      >Reveal and Copy hide the value again after 30 seconds</span
    >
  </div>

  {#if oauth}
    <div class="frow">
      <label class="lbl" for="{id}-refresh">Refresh token</label>
      <input
        id="{id}-refresh"
        class="input mono"
        type="password"
        autocomplete="off"
        spellcheck="false"
        bind:value={refreshToken}
      />
    </div>
    <div class="frow">
      <label class="lbl" for="{id}-expires">Expires</label>
      <input
        id="{id}-expires"
        class="input"
        type="datetime-local"
        bind:value={expires}
      />
    </div>
  {/if}

  <div class="frow">
    <label class="lbl" for="{id}-description">Description</label>
    <input
      id="{id}-description"
      class="input"
      type="text"
      bind:value={description}
    />
  </div>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="buttons">
    <Button variant="primary" type="submit" disabled={busy}>Save secret</Button>
    <Button onclick={cancel}>Cancel</Button>
  </div>
</form>

<style>
  .page {
    max-width: 640px;
    padding: var(--s-6) var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    overflow: auto;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
    margin-bottom: var(--s-3);
  }

  h1 {
    font-size: 20px;
    line-height: 28px;
    font-weight: 500;
  }

  .head p {
    color: var(--text-2);
  }

  .frow {
    display: grid;
    grid-template-columns: 140px minmax(0, 1fr);
    align-items: center;
    row-gap: var(--s-1);
  }

  .lbl {
    color: var(--text-2);
  }

  .pick {
    display: flex;
    gap: var(--s-2);
    align-items: center;
  }

  .input {
    height: 36px;
    width: 100%;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .input:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .input:disabled {
    color: var(--text-3);
  }

  .input[aria-invalid="true"] {
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .mono {
    font-family: var(--font-mono);
  }

  .meta,
  .problem {
    grid-column: 2;
    font-size: 12px;
  }

  .meta {
    color: var(--text-3);
  }

  .problem,
  .error {
    color: var(--negative-text);
  }

  .buttons {
    display: flex;
    gap: var(--s-2);
    padding-left: 140px;
  }

  .error {
    padding-left: 140px;
  }
</style>
