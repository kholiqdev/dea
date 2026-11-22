<!--
  The request's headers, then those Dea adds when it sends: a header above
  with the same name replaces one of these (F1-REQ-02 #5).
-->
<script lang="ts">
  import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
  import type { RequestDraft } from "../state/drafts.svelte";
  import type { VariableLookup } from "../state/variables.svelte";
  import KeyValueTable, { sendLabel } from "./KeyValueTable.svelte";

  let {
    draft,
    variables,
    readonly,
    highlight = null,
  }: {
    draft: RequestDraft;
    variables: VariableLookup;
    readonly: boolean;
    /** The row of the table a warning points at. */
    highlight?: number | null;
  } = $props();

  const automatic = $derived(draft.inspection?.automaticHeaders ?? []);
</script>

<KeyValueTable
  rows={draft.draft.headers ?? []}
  label="Headers"
  addLabel="Add header"
  checkboxLabel={sendLabel}
  {variables}
  {highlight}
  disabled={readonly}
  onchange={(rows: KeyValue[]) => draft.setHeaders(rows)}
/>
<section class="group">
  <h3>Automatic headers</h3>
  <p class="hint">
    Added when the request is sent. A header with the same name above replaces
    it.
  </p>
  {#if automatic.length > 0}
    <ul class="card" aria-label="Automatic headers">
      {#each automatic as header (header.name)}
        <li class:replaced={header.replaced}>
          {#if header.replaced}
            <s class="name">{header.name}</s><s
              class="value"
              class:later={header.value === null}
              >{header.value ?? "Calculated when sent"}</s
            ><span class="tag">Replaced</span>
          {:else}
            <span class="name">{header.name}</span><span
              class="value"
              class:later={header.value === null}
              >{header.value ?? "Calculated when sent"}</span
            >
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  h3 {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .hint {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .card {
    margin: 0;
    padding: 0;
    list-style: none;
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  li {
    height: 36px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr) 64px;
    align-items: center;
    column-gap: var(--s-3);
    padding: 0 var(--s-3);
  }

  li + li {
    border-top: 1px solid var(--line);
  }

  .name,
  .value {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
  }

  .name {
    color: var(--text-1);
  }

  .value {
    color: var(--text-2);
  }

  .later {
    font-family: var(--font-ui);
    color: var(--text-3);
  }

  .replaced .name,
  .replaced .value {
    color: var(--text-3);
  }

  .tag {
    justify-self: end;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }
</style>
