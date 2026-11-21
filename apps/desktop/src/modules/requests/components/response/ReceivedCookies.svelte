<!--
  The cookies a response set, and the way to the cookie manager for the
  ones Dea keeps.
-->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import { shellApi } from "$app/contributions/shell-api";
  import type { CookieInfo } from "$lib/ipc/bindings/CookieInfo";
  import { formatCookieDate } from "../../lib/format";

  let { cookies }: { cookies: CookieInfo[] } = $props();

  const shell = shellApi();
</script>

<div class="cookies">
  <div class="bar">
    {#if cookies.length === 0}
      <p class="empty">This response set no cookies.</p>
    {/if}
    <span class="spacer"></span>
    <Button
      variant="ghost"
      size="small"
      onclick={() => void shell.runCommand("requests.manageCookies")}
    >
      Manage cookies
    </Button>
  </div>
  {#if cookies.length > 0}
    <table aria-label="Received cookies">
      <thead>
        <tr>
          <th scope="col">Name</th>
          <th scope="col">Value</th>
          <th scope="col">Domain</th>
          <th scope="col">Path</th>
          <th scope="col">Expires</th>
          <th scope="col">Flags</th>
        </tr>
      </thead>
      <tbody>
        {#each cookies as cookie, i (i)}
          <tr>
            <td>{cookie.name}</td>
            <td>{cookie.value}</td>
            <td>{cookie.domain}</td>
            <td>{cookie.path}</td>
            <td>{formatCookieDate(cookie.expires)}</td>
            <td class="flags">
              {#if cookie.secure}<span class="flag">Secure</span>{/if}
              {#if cookie.httpOnly}<span class="flag">HttpOnly</span>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .cookies {
    padding: var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: var(--s-3);
  }

  .empty {
    margin: 0;
    color: var(--text-3);
  }

  .spacer {
    flex: 1;
  }

  table {
    width: 100%;
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

  .flags {
    display: flex;
    gap: var(--s-2);
    flex-wrap: wrap;
  }

  .flag {
    color: var(--text-2);
  }
</style>
