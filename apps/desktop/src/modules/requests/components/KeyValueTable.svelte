<!--
  Rows of name, value and description, as params, headers and form fields
  are kept. Values show {{variables}}. With `checkboxLabel` each row has a
  checkbox that says whether it is sent; with `lockedNames` the names are
  given, so rows are neither typed nor added nor removed.
-->
<script module lang="ts">
  import type { KeyValue } from "$lib/ipc/bindings/KeyValue";

  /** The checkbox of a row that is only sent while it is checked. */
  export function sendLabel(row: KeyValue, index: number): string {
    return `Send ${row.name || `row ${index + 1}`}`;
  }
</script>

<script lang="ts">
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import { tick } from "svelte";
  import type { VariableLookup } from "../state/variables.svelte";
  import { sessionView } from "../state/session-view.svelte";
  import DescriptionToggle from "./DescriptionToggle.svelte";
  import VariableField from "./VariableField.svelte";

  let {
    rows,
    label,
    keyLabel = "Key",
    addLabel,
    checkboxLabel,
    variables,
    onchange,
    lockedNames = false,
    disabled = false,
  }: {
    rows: KeyValue[];
    /** The table's accessible name. */
    label: string;
    keyLabel?: string;
    addLabel: string;
    checkboxLabel?: (row: KeyValue, index: number) => string;
    variables: VariableLookup;
    onchange: (rows: KeyValue[]) => void;
    lockedNames?: boolean;
    disabled?: boolean;
  } = $props();

  let body = $state<HTMLElement>();

  const showDescriptions = $derived(sessionView.descriptions);
  const hasDescriptions = $derived(rows.some((row) => row.description));

  const named = (row: KeyValue, i: number) => row.name || `row ${i + 1}`;

  function update(index: number, patch: Partial<KeyValue>): void {
    onchange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  async function add(): Promise<void> {
    onchange([...rows, { name: "", value: "" }]);
    await tick();
    body?.querySelector<HTMLInputElement>("tr:last-child input.key")?.focus();
  }
</script>

<div class="card">
  <table aria-label={label} class:checks={!!checkboxLabel}>
    <colgroup>
      {#if checkboxLabel}<col class="narrow" />{/if}
      <col class={showDescriptions ? "key-wide" : "key-narrow"} />
      <col />
      {#if showDescriptions}<col class="description-col" />{/if}
      <col class="narrow" />
    </colgroup>
    <thead>
      <tr>
        {#if checkboxLabel}<td></td>{/if}
        <th scope="col">{keyLabel}</th>
        <th scope="col">Value</th>
        {#if showDescriptions}<th scope="col">Description</th>{/if}
        <td class="center"><DescriptionToggle {hasDescriptions} /></td>
      </tr>
    </thead>
    <tbody bind:this={body}>
      {#each rows as row, i (i)}
        <tr class:off={row.enabled === false}>
          {#if checkboxLabel}
            <td class="center">
              <input
                type="checkbox"
                class="check"
                aria-label={checkboxLabel(row, i)}
                checked={row.enabled !== false}
                {disabled}
                onchange={(e) =>
                  update(i, { enabled: e.currentTarget.checked })}
              />
            </td>
          {/if}
          <td>
            {#if lockedNames}
              <span class="name">{row.name}</span>
            {:else}
              <input
                class="cell key"
                aria-label="{keyLabel} of row {i + 1}"
                value={row.name}
                autocomplete="off"
                spellcheck="false"
                {disabled}
                oninput={(e) => update(i, { name: e.currentTarget.value })}
              />
            {/if}
          </td>
          <td class="value">
            <VariableField
              value={row.value}
              label="Value of {named(row, i)}"
              mono
              {variables}
              {disabled}
              oninput={(value: string) => update(i, { value })}
            />
          </td>
          {#if showDescriptions}
            <td>
              <input
                class="cell description"
                aria-label="Description of {named(row, i)}"
                value={row.description ?? ""}
                {disabled}
                oninput={(e) =>
                  update(i, {
                    description: e.currentTarget.value || undefined,
                  })}
              />
            </td>
          {/if}
          <td class="center">
            {#if !lockedNames}
              <button
                type="button"
                class="remove"
                aria-label="Remove {named(row, i)}"
                title="Remove {named(row, i)}"
                {disabled}
                onclick={() => onchange(rows.filter((_, j) => j !== i))}
              >
                <X size={14} strokeWidth={1.5} />
              </button>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if !lockedNames}
    <button type="button" class="add" {disabled} onclick={() => void add()}>
      <span class="plus"><Plus size={16} strokeWidth={1.5} /></span>{addLabel}
    </button>
  {/if}
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

  /* Shares of the width between the checkbox and the last column. */
  table {
    --fixed: 36px;
  }

  table.checks {
    --fixed: 72px;
  }

  .key-narrow {
    width: calc((100% - var(--fixed)) * 0.35);
  }

  .key-wide {
    width: calc((100% - var(--fixed)) * 0.28);
  }

  .description-col {
    width: calc((100% - var(--fixed)) * 0.28);
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
    height: 36px;
    padding: 0 var(--s-1);
    border-top: 1px solid var(--line);
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

  .name,
  .cell {
    display: block;
    width: 100%;
    height: 28px;
    padding: 0 var(--s-2);
    font-family: var(--font-mono);
    line-height: 28px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell {
    border: 0;
    border-radius: var(--r-small);
    background: none;
    outline: none;
  }

  .cell.description {
    font-family: var(--font-ui);
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

  /* The value field draws no box until it is pointed at or typed in. */
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
  .off .name,
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
