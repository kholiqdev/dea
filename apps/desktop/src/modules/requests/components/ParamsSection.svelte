<!--
  Path variables, named by the URL, and the query params, which the URL bar
  shows too (F1-REQ-02 #3 and #4).
-->
<script lang="ts">
  import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
  import { pathVariableNames, syncPathVariables } from "../lib/url";
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

  const pathVariables = $derived(
    syncPathVariables(
      draft.draft.pathVariables ?? [],
      pathVariableNames(draft.draft.url),
    ),
  );
</script>

{#if pathVariables.length > 0}
  <section class="group">
    <h3>Path variables</h3>
    <KeyValueTable
      rows={pathVariables}
      label="Path variables"
      addLabel=""
      lockedNames
      {variables}
      disabled={readonly}
      onchange={(rows: KeyValue[]) => draft.setPathVariables(rows)}
    />
  </section>
{/if}
<section class="group">
  <h3>Query parameters</h3>
  <KeyValueTable
    rows={draft.draft.params ?? []}
    label="Query parameters"
    addLabel="Add parameter"
    checkboxLabel={sendLabel}
    {variables}
    {highlight}
    disabled={readonly}
    onchange={(rows: KeyValue[]) => draft.setParams(rows)}
  />
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
</style>
