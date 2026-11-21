<!--
  The headers of a response as they came, duplicates and order kept, under
  the hops that redirected to it.
-->
<script lang="ts">
  import type { HeaderRow } from "$lib/ipc/bindings/HeaderRow";
  import type { RedirectInfo } from "$lib/ipc/bindings/RedirectInfo";

  let {
    headers,
    redirects,
  }: { headers: HeaderRow[]; redirects: RedirectInfo[] } = $props();
</script>

<div class="headers">
  {#if redirects.length > 0}
    <div class="redirects">
      <p class="title">
        Redirected {redirects.length}
        {redirects.length === 1 ? "time" : "times"}
      </p>
      <ul>
        {#each redirects as hop, i (i)}
          <li>{hop.status} {hop.url}</li>
        {/each}
      </ul>
    </div>
  {/if}
  <table aria-label="Response headers">
    <thead>
      <tr>
        <th scope="col">Name</th>
        <th scope="col">Value</th>
      </tr>
    </thead>
    <tbody>
      {#each headers as header, i (i)}
        <tr>
          <td class="name">{header.name}</td>
          <td class="value">{header.value}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .headers {
    padding: var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
  }

  .redirects {
    color: var(--text-2);
  }

  .title {
    margin: 0 0 var(--s-2);
    color: var(--text-1);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
  }

  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
  }

  th {
    height: 32px;
    padding: 0 var(--s-2);
    text-align: left;
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
  }

  td {
    padding: var(--s-1) var(--s-2);
    vertical-align: top;
    box-shadow: inset 0 1px 0 var(--line);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
  }

  .name {
    width: 30%;
    color: var(--text-2);
  }

  .value {
    color: var(--text-1);
  }
</style>
