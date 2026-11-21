<!--
  The cookie manager (F1-REQ-09): the cookies of an environment, by domain,
  to edit or delete. Cookies are kept per environment.
-->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { untrack } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import Menu, { type MenuItem } from "@dea/ui/Menu.svelte";
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import type { CookieInfo } from "$lib/ipc/bindings/CookieInfo";
  import { errorMessage } from "$lib/ipc/errors";
  import CookieDialog from "./components/CookieDialog.svelte";
  import { formatCookieDate } from "./lib/format";
  import { CookiesState } from "./state/cookies.svelte";
  import { cookiesVersion } from "./state/exchange.svelte";

  let { tab }: { tab: { id: string; params: unknown } } = $props();

  const project = projectApi();
  const shell = shellApi();
  const jar = new CookiesState(
    untrack(
      () =>
        (tab.params as { environment?: string | null } | null)?.environment ??
        null,
    ),
  );

  let environmentOpen = $state(false);
  let domainOpen = $state(false);
  let editing = $state<CookieInfo | null>(null);
  let confirming = $state(false);

  const environments = $derived(project.info?.environments ?? []);
  const label = $derived(jar.environment ?? "No environment");

  const environmentItems = $derived.by((): MenuItem[] => [
    ...environments.map((env) => ({
      id: `env:${env.name}`,
      label: env.name,
      checked: env.name === jar.environment,
      ...(env.production ? { badge: "production" } : {}),
      onselect: () => void run(() => jar.chooseEnvironment(env.name)),
    })),
    {
      id: "env:none",
      label: "No environment",
      checked: jar.environment === null,
      onselect: () => void run(() => jar.chooseEnvironment(null)),
    },
  ]);

  const domainItems = $derived.by((): MenuItem[] =>
    jar.domains.map((domain) => ({
      id: `domain:${domain}`,
      label: domain,
      checked: domain === jar.domain,
      onselect: () => (jar.chosen = domain),
    })),
  );

  // Reads the jar when the tab opens, after each send and when the tab is
  // shown again.
  let seenVersion: number | null = null;
  let wasActive = false;
  $effect(() => {
    const version = cookiesVersion();
    const active = shell.activeTab()?.id === tab.id;
    untrack(() => {
      if (version !== seenVersion || (active && !wasActive))
        void run(() => jar.load());
      seenVersion = version;
      wasActive = active;
    });
  });

  async function run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
    } catch (e) {
      shell.toast(errorMessage(e));
    }
  }

  const reload = () => run(() => jar.load());

  async function remove(cookie: CookieInfo): Promise<void> {
    const environment = jar.environment;
    try {
      await jar.remove(cookie, environment);
    } catch (e) {
      shell.toast(errorMessage(e));
      return;
    }
    shell.toast(`Deleted cookie ${cookie.name}`, {
      action: {
        label: "Undo",
        run: () =>
          void run(async () => {
            await jar.restore(cookie, environment);
            await jar.load();
          }),
      },
    });
    await reload();
  }

  async function save(cookie: CookieInfo): Promise<void> {
    if (!editing) return;
    const { domain, path, name } = editing;
    // A refusal stays in the dialog; a later failed read does not.
    await jar.save({ domain, path, name }, cookie);
    editing = null;
    await reload();
  }

  async function removeAll(): Promise<void> {
    const domain = jar.domain;
    confirming = false;
    if (domain === null) return;
    try {
      await jar.removeDomain(domain);
    } catch (e) {
      shell.toast(errorMessage(e));
      return;
    }
    await reload();
  }
</script>

<div class="cookies">
  <header class="bar">
    <h1>Cookies</h1>
    <Menu
      bind:open={environmentOpen}
      items={environmentItems}
      menuLabel="Environment"
      title="Environment"
    >
      {#snippet trigger()}
        <span>{label}</span>
        <ChevronDown size={14} strokeWidth={1.5} />
      {/snippet}
    </Menu>
    <Menu
      bind:open={domainOpen}
      items={domainItems}
      menuLabel="Domain"
      title="Domain"
    >
      {#snippet trigger()}
        <span>{jar.domain ?? "No domain"}</span>
        <ChevronDown size={14} strokeWidth={1.5} />
      {/snippet}
    </Menu>
    <span class="spacer"></span>
    <Button
      variant="danger"
      size="small"
      disabled={jar.domain === null}
      onclick={() => (confirming = true)}
    >
      Delete all for this domain…
    </Button>
  </header>

  {#if jar.loaded && jar.cookies.length === 0}
    <p class="empty">
      No cookies in {label} yet. Cookies that responses set appear here.
    </p>
  {:else if jar.rows.length > 0}
    <table aria-label="Cookies">
      <thead>
        <tr>
          <th scope="col">Name</th>
          <th scope="col">Value</th>
          <th scope="col">Expires</th>
          <th scope="col"><span class="visually-hidden">Actions</span></th>
        </tr>
      </thead>
      <tbody>
        {#each jar.rows as cookie (`${cookie.domain}\u0000${cookie.path}\u0000${cookie.name}`)}
          <tr>
            <td>{cookie.name}</td>
            <td class="value">{cookie.value}</td>
            <td>{formatCookieDate(cookie.expires)}</td>
            <td class="actions">
              <button
                type="button"
                class="icon"
                aria-label="Edit cookie {cookie.name}"
                onclick={() => (editing = cookie)}
              >
                <Pencil size={14} strokeWidth={1.5} />
              </button>
              <button
                type="button"
                class="icon"
                aria-label="Delete cookie {cookie.name}"
                onclick={() => void remove(cookie)}
              >
                <Trash2 size={14} strokeWidth={1.5} />
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <p class="note">Cookies are kept per environment and domain</p>
</div>

{#if editing}
  <CookieDialog
    cookie={editing}
    onsave={save}
    oncancel={() => (editing = null)}
  />
{/if}

{#if confirming && jar.domain !== null}
  <Dialog
    title="Delete {jar.rows.length} cookies for {jar.domain}?"
    oncancel={() => (confirming = false)}
  >
    <p class="text">Requests in {label} stop sending them.</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirming = false)}>
        Cancel
      </Button>
      <Button variant="danger-solid" onclick={() => void removeAll()}>
        Delete cookies
      </Button>
    {/snippet}
  </Dialog>
{/if}

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

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  .spacer {
    flex: 1;
  }

  .empty,
  .text {
    margin: 0;
    color: var(--text-2);
  }

  .note {
    margin: 0;
    font-size: 12px;
    color: var(--text-3);
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
    vertical-align: middle;
    box-shadow: inset 0 1px 0 var(--line);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-1);
  }

  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: var(--r-small);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }

  .icon:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .icon:focus-visible {
    outline: 2px solid var(--focus);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
