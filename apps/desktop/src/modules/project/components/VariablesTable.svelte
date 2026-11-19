<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import Plus from "@lucide/svelte/icons/plus";
  import type { EnvironmentDraft } from "../state/environments.svelte";
  import { vaultReferences } from "../state/environments.svelte";

  let {
    draft,
    secretNames,
    readonly = false,
  }: {
    draft: EnvironmentDraft;
    secretNames: string[];
    readonly?: boolean;
  } = $props();

  const variables = $derived(draft.draft.variables ?? []);
  const unknownSecret = (value: string) =>
    vaultReferences(value).some((name) => !secretNames.includes(name));
</script>

<table class="table" aria-label="Variables">
  <thead>
    <tr>
      <th scope="col">Name</th>
      <th scope="col">Value</th>
      <th scope="col">Description</th>
      <th scope="col"><span class="sr">Remove</span></th>
    </tr>
  </thead>
  <tbody>
    {#each variables as variable, i (i)}
      <tr>
        <td>
          <input
            class="cell mono"
            aria-label="Name of variable {i + 1}"
            bind:value={variable.name}
            {readonly}
          />
        </td>
        <td>
          <input
            class="cell mono"
            aria-label="Value of {variable.name || `variable ${i + 1}`}"
            bind:value={variable.value}
            {readonly}
          />
          {#if unknownSecret(variable.value)}
            <span class="warn">Not in this project's Vault yet</span>
          {/if}
        </td>
        <td>
          <input
            class="cell"
            aria-label="Description of {variable.name || `variable ${i + 1}`}"
            value={variable.description ?? ""}
            oninput={(e) =>
              (variable.description = e.currentTarget.value || undefined)}
            {readonly}
          />
        </td>
        <td class="remove">
          {#if !readonly}
            <button
              type="button"
              class="iconbtn"
              aria-label="Remove {variable.name || `variable ${i + 1}`}"
              onclick={() => draft.removeVariable(i)}
            >
              <X size={14} strokeWidth={1.5} />
            </button>
          {/if}
        </td>
      </tr>
    {/each}
    {#if !readonly}
      <tr>
        <td colspan="4">
          <button type="button" class="add" onclick={() => draft.addVariable()}>
            <Plus size={14} strokeWidth={1.5} />Add variable
          </button>
        </td>
      </tr>
    {/if}
  </tbody>
</table>

<style>
  .table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
  }

  th,
  td {
    height: 40px;
    padding: 0 var(--s-3);
    text-align: left;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }

  th:first-child,
  td:first-child {
    padding-left: var(--s-4);
  }

  th {
    height: 32px;
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
  }

  .cell {
    width: 100%;
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-1);
  }

  .cell:hover:not([readonly]),
  .cell:focus-visible {
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    outline: none;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .warn {
    display: block;
    padding: 0 var(--s-2) var(--s-1);
    font-size: 12px;
    color: var(--warning-text);
  }

  .remove {
    width: 40px;
  }

  .iconbtn,
  .add {
    border: 0;
    background: none;
    color: var(--text-3);
  }

  .iconbtn {
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--r-small);
  }

  .add {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0;
  }

  .iconbtn:hover,
  .add:hover {
    color: var(--text-1);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
