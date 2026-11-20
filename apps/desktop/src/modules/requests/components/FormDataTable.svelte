<!--
  The fields of a multipart body: each is text or a file. The checkbox says
  whether the field is sent, as in the other tables.
-->
<script lang="ts">
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import { tick } from "svelte";
  import type { FormField } from "$lib/ipc/bindings/FormField";
  import type { VariableLookup } from "../state/variables.svelte";
  import FileChoice from "./FileChoice.svelte";
  import VariableField from "./VariableField.svelte";

  let {
    fields,
    variables,
    disabled = false,
    onchange,
    onerror,
  }: {
    fields: FormField[];
    variables: VariableLookup;
    disabled?: boolean;
    onchange: (fields: FormField[]) => void;
    onerror?: (message: string) => void;
  } = $props();

  let body = $state<HTMLElement>();

  const named = (field: FormField, i: number) => field.name || `row ${i + 1}`;
  const isFile = (field: FormField) => field.file !== undefined;

  function update(index: number, next: (field: FormField) => FormField): void {
    onchange(fields.map((f, i) => (i === index ? next(f) : f)));
  }

  function setType(index: number, type: string): void {
    update(index, (field) => {
      const next: FormField = { ...field };
      delete next.value;
      delete next.file;
      return type === "file" ? { ...next, file: "" } : { ...next, value: "" };
    });
  }

  async function add(): Promise<void> {
    onchange([...fields, { name: "", value: "" }]);
    await tick();
    body?.querySelector<HTMLInputElement>("tr:last-child input.key")?.focus();
  }
</script>

<div class="card">
  <table aria-label="Form data">
    <colgroup>
      <col class="narrow" />
      <col class="key-col" />
      <col class="type-col" />
      <col class="value-col" />
      <col class="key-col" />
      <col class="narrow" />
    </colgroup>
    <thead>
      <tr>
        <td></td>
        <th scope="col">Key</th>
        <th scope="col">Type</th>
        <th scope="col">Value</th>
        <th scope="col">Description</th>
        <td></td>
      </tr>
    </thead>
    <tbody bind:this={body}>
      {#each fields as field, i (i)}
        <tr class:off={field.enabled === false}>
          <td class="center">
            <input
              type="checkbox"
              class="check"
              aria-label="Send {named(field, i)}"
              checked={field.enabled !== false}
              {disabled}
              onchange={(e) =>
                update(i, (f) => ({ ...f, enabled: e.currentTarget.checked }))}
            />
          </td>
          <td>
            <input
              class="cell key"
              aria-label="Key of row {i + 1}"
              value={field.name}
              autocomplete="off"
              spellcheck="false"
              {disabled}
              oninput={(e) =>
                update(i, (f) => ({ ...f, name: e.currentTarget.value }))}
            />
          </td>
          <td>
            <select
              class="cell type"
              aria-label="Type of {named(field, i)}"
              value={isFile(field) ? "file" : "text"}
              {disabled}
              onchange={(e) => setType(i, e.currentTarget.value)}
            >
              <option value="text">Text</option>
              <option value="file">File</option>
            </select>
          </td>
          <td class="value">
            {#if isFile(field)}
              <FileChoice
                value={field.file ?? ""}
                {disabled}
                {onerror}
                onpick={(file: string) => update(i, (f) => ({ ...f, file }))}
              />
            {:else}
              <VariableField
                value={field.value ?? ""}
                label="Value of {named(field, i)}"
                mono
                {variables}
                {disabled}
                oninput={(value: string) => update(i, (f) => ({ ...f, value }))}
              />
            {/if}
          </td>
          <td>
            <input
              class="cell description"
              aria-label="Description of {named(field, i)}"
              value={field.description ?? ""}
              {disabled}
              oninput={(e) =>
                update(i, (f) => ({
                  ...f,
                  description: e.currentTarget.value || undefined,
                }))}
            />
          </td>
          <td class="center">
            <button
              type="button"
              class="remove"
              aria-label="Remove {named(field, i)}"
              title="Remove {named(field, i)}"
              {disabled}
              onclick={() => onchange(fields.filter((_, j) => j !== i))}
            >
              <X size={14} strokeWidth={1.5} />
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <button type="button" class="add" {disabled} onclick={() => void add()}>
    <span class="plus"><Plus size={16} strokeWidth={1.5} /></span>Add field
  </button>
</div>

<style>
  .card {
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    overflow: hidden;
  }

  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
  }

  .narrow {
    width: 36px;
  }

  .key-col {
    width: 24%;
  }

  .type-col {
    width: 96px;
  }

  th,
  thead td {
    height: 32px;
    padding: 0 var(--s-2);
    text-align: left;
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-3);
  }

  tbody td {
    min-height: 36px;
    padding: 0 var(--s-1);
    border-top: 1px solid var(--line);
    vertical-align: middle;
  }

  .center {
    text-align: center;
  }

  .check {
    width: 16px;
    height: 16px;
    margin: 0;
    vertical-align: middle;
    accent-color: var(--accent);
  }

  .cell {
    display: block;
    width: 100%;
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    outline: none;
    font-family: var(--font-mono);
    line-height: 28px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell.type,
  .cell.description {
    font-family: var(--font-ui);
  }

  .cell.description {
    color: var(--text-3);
  }

  .cell:hover:not(:disabled),
  .cell:focus-visible {
    background: var(--bg-1);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }

  .cell:focus-visible {
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .cell:disabled {
    opacity: 0.5;
  }

  .value :global(.field) {
    background: none;
    box-shadow: none;
  }

  .value :global(.field:hover),
  .value :global(.field:focus-within) {
    background: var(--bg-1);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }

  .value :global(.field:focus-within) {
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .off .key,
  .off .value {
    color: var(--text-3);
    opacity: 0.7;
  }

  .remove {
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-3);
    opacity: 0;
  }

  tr:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }

  .remove:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .add {
    width: 100%;
    height: 36px;
    display: flex;
    align-items: center;
    gap: var(--s-1);
    padding: 0 var(--s-1);
    border: 0;
    border-top: 1px solid var(--line);
    background: none;
    color: var(--text-3);
    text-align: left;
  }

  .add:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .add:disabled {
    opacity: 0.5;
  }

  .plus {
    width: 28px;
    display: flex;
    justify-content: center;
  }
</style>
