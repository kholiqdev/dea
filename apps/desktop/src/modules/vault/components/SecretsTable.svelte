<!-- The secrets of one environment, one row each. -->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
  import type { SecretRow } from "$lib/ipc/bindings/SecretRow";
  import { expiresIn, relativeTime } from "$lib/time/relative";
  import { KIND_LABELS } from "../kinds";
  import { usedBy } from "../state/secrets.svelte";

  let {
    rows,
    info,
    selected,
    onselect,
    onadd,
  }: {
    rows: SecretRow[];
    info: ProjectInfo;
    selected: SecretRow | null;
    onselect: (row: SecretRow) => void;
    /** Add the value of a declared secret that has none here. */
    onadd: (row: SecretRow) => void;
  } = $props();

  const now = new Date();

  function variables(row: SecretRow): string {
    const n = usedBy(info, row.environment, row.name).length;
    if (n === 0) return "—";
    return n === 1 ? "1 variable" : `${n} variables`;
  }
</script>

<table class="table" aria-label="Secrets">
  <thead>
    <tr>
      <th scope="col">Name</th>
      <th scope="col">Type</th>
      <th scope="col">Used by</th>
      <th scope="col">Expires</th>
      <th scope="col">Updated</th>
    </tr>
  </thead>
  <tbody>
    {#each rows as row (row.name)}
      <tr class={{ selected: row === selected }}>
        <td>
          <button
            type="button"
            class="name"
            aria-current={row === selected ? "true" : undefined}
            onclick={() => onselect(row)}>{row.name}</button
          >
        </td>
        <td>{KIND_LABELS[row.kind]}</td>
        <td>{variables(row)}</td>
        <td>{row.expiresAt ? expiresIn(row.expiresAt, now) : "—"}</td>
        <td>
          {#if row.missing}
            <span class="missing">Missing</span>
            <Button size="small" onclick={() => onadd(row)}>Add value</Button>
          {:else}
            {row.updatedAt ? relativeTime(row.updatedAt, now) : "—"}
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  .table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    padding: var(--s-2) var(--s-3);
    text-align: left;
    box-shadow: inset 0 -1px 0 var(--line);
    color: var(--text-2);
  }

  th {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
  }

  tr.selected td {
    background: var(--bg-3);
  }

  .name {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-1);
    font: 400 13px/20px var(--font-mono);
  }

  .missing {
    margin-right: var(--s-2);
    color: var(--negative-text);
  }
</style>
