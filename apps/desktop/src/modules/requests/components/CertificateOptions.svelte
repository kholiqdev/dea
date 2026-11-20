<!--
  A collection's certificates: extra authorities to trust, and a client
  certificate whose private key is a Vault secret, never a file.
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import Plus from "@lucide/svelte/icons/plus";
  import { onMount } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import type { TlsSettings } from "$lib/ipc/bindings/TlsSettings";
  import { vaultSecrets } from "$lib/ipc/vault";
  import FileChoice from "./FileChoice.svelte";

  let {
    tls,
    onchange,
    onerror,
    disabled = false,
  }: {
    tls: TlsSettings | undefined;
    onchange: (tls: TlsSettings) => void;
    onerror?: (message: string) => void;
    disabled?: boolean;
  } = $props();

  const uid = $props.id();
  let names = $state<string[]>([]);

  // Only the names of the secrets, never their values.
  onMount(() => {
    vaultSecrets().then(
      (rows) => {
        names = [...new Set((rows ?? []).map((r) => r.name))].sort();
      },
      () => {
        names = [];
      },
    );
  });

  const authorities = $derived(tls?.caCertificates ?? []);
  const client = $derived(tls?.clientCertificate);
  const VAULT = /^\{\{vault:(.+)\}\}$/;
  const keyName = $derived(VAULT.exec(client?.key ?? "")?.[1] ?? "");
  // A key naming a secret that is not declared stays choosable.
  const choices = $derived(
    keyName && !names.includes(keyName) ? [...names, keyName] : names,
  );

  const hint = $derived(
    !client
      ? null
      : client.certificate !== "" && client.key === ""
        ? "Choose a key from Vault to use this certificate"
        : client.certificate === "" && client.key !== ""
          ? "Choose a certificate file to use this key"
          : null,
  );

  function setAuthorities(next: string[]): void {
    onchange({ ...tls, caCertificates: next });
  }

  function removeClient(): void {
    const rest = { ...tls };
    delete rest.clientCertificate;
    onchange(rest);
  }

  function setClient(patch: { certificate?: string; key?: string }): void {
    onchange({
      ...tls,
      clientCertificate: {
        certificate: client?.certificate ?? "",
        key: client?.key ?? "",
        ...patch,
      },
    });
  }
</script>

<div class="card">
  <div class="block">
    <h3>CA certificate</h3>
    {#each authorities as authority, i (i)}
      <div class="line">
        <FileChoice
          value={authority}
          label="CA certificate {i + 1}"
          {disabled}
          {onerror}
          onpick={(value: string) =>
            setAuthorities(authorities.map((a, j) => (j === i ? value : a)))}
        />
        <button
          type="button"
          class="remove"
          aria-label="Remove CA certificate {i + 1}"
          {disabled}
          onclick={() => setAuthorities(authorities.filter((_, j) => j !== i))}
        >
          <X size={14} strokeWidth={1.5} />
        </button>
      </div>
    {/each}
    <button
      type="button"
      class="add"
      {disabled}
      onclick={() => setAuthorities([...authorities, ""])}
    >
      <Plus size={16} strokeWidth={1.5} />Add CA certificate
    </button>
  </div>
  <div class="block">
    <h3>Client certificate</h3>
    <FileChoice
      value={client?.certificate ?? ""}
      label="Client certificate"
      {disabled}
      {onerror}
      onpick={(certificate: string) => setClient({ certificate })}
    />
    <label class="key" for="{uid}-key">
      Key from Vault
      <select
        id="{uid}-key"
        value={keyName}
        {disabled}
        onchange={(e) =>
          setClient({
            key: e.currentTarget.value
              ? `{{vault:${e.currentTarget.value}}}`
              : "",
          })}
      >
        <option value="">None</option>
        {#each choices as name (name)}
          <option value={name}>{name}</option>
        {/each}
      </select>
    </label>
    {#if client && (client.certificate !== "" || client.key !== "")}
      <div class="remove-client">
        <Button variant="ghost" size="small" {disabled} onclick={removeClient}
          >Remove client certificate</Button
        >
      </div>
    {/if}
    {#if hint}<p class="hint">{hint}</p>{/if}
  </div>
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-3);
  }

  .block + .block {
    border-top: 1px solid var(--line);
  }

  h3 {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .line :global(.choice) {
    flex: 1;
  }

  .remove,
  .add {
    display: inline-flex;
    align-items: center;
    border: 0;
    background: none;
    color: var(--text-2);
  }

  .remove {
    width: 28px;
    height: 28px;
    justify-content: center;
    border-radius: var(--r-small);
  }

  .add {
    align-self: flex-start;
    height: 28px;
    gap: var(--s-1);
    padding: 0 var(--s-2);
    border-radius: var(--r-small);
  }

  .remove:hover:not(:disabled),
  .add:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .key {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 0 var(--s-2);
    color: var(--text-2);
  }

  .remove-client {
    align-self: flex-start;
  }

  .hint {
    margin: 0;
    padding: 0 var(--s-2);
    font-size: 12px;
    line-height: 16px;
    color: var(--warning-text);
  }

  select {
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-1);
  }
</style>
