<script lang="ts">
  import Folder from "@lucide/svelte/icons/folder";
  import FolderX from "@lucide/svelte/icons/folder-x";
  import Button from "@dea/ui/Button.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import type { RecentEntry } from "$lib/ipc/bindings/RecentEntry";
  import {
    openProject,
    pickFolder,
    recentProjects,
    removeRecent,
    windowActions,
  } from "../api";
  import { relativeDay } from "../state/welcome.svelte";

  let { onerror }: { onerror: (message: string) => void } = $props();

  let entries = $state.raw<RecentEntry[]>([]);
  const now = Math.floor(Date.now() / 1000);

  async function load(): Promise<void> {
    entries = await recentProjects().catch(() => []);
  }
  void load();

  async function attempt(action: () => Promise<unknown>): Promise<void> {
    try {
      await action();
    } catch (e) {
      onerror(errorMessage(e));
    }
  }

  const open = (root: string) =>
    attempt(async () => {
      await openProject(root);
      windowActions.reload();
    });

  const locate = () =>
    attempt(async () => {
      const root = await pickFolder();
      if (!root) return;
      await openProject(root);
      windowActions.reload();
    });

  const remove = (root: string) =>
    attempt(async () => {
      await removeRecent(root);
      await load();
    });

  const count = (n: number) => (n === 1 ? "1 request" : `${n} requests`);
</script>

{#if entries.length > 0}
  <section class="group">
    <div class="head">
      <h3 class="field-label" id="recent-label">Recent projects</h3>
      <span class="meta">The 10 most recent are kept</span>
    </div>
    <ul class="card list" aria-labelledby="recent-label">
      {#each entries as entry (entry.root)}
        <li class="row">
          {#if entry.missing}
            <span class="entry missing">
              <FolderX size={16} strokeWidth={1.5} />
              <span class="names">
                <span class="name">{entry.name}</span>
                <span class="path">{entry.root}</span>
              </span>
              <span class="meta warn">Folder not found</span>
            </span>
            <span class="actions">
              <Button size="small" onclick={locate}>Locate</Button>
              <Button
                size="small"
                variant="ghost"
                onclick={() => remove(entry.root)}>Remove from list</Button
              >
            </span>
          {:else}
            <button
              type="button"
              class="entry"
              onclick={() => open(entry.root)}
            >
              <Folder size={16} strokeWidth={1.5} />
              <span class="names">
                <span class="name">{entry.name}</span>
                <span class="path">{entry.root}</span>
              </span>
              <span class="meta mono">{count(entry.requestCount)}</span>
              <span class="meta when">{relativeDay(entry.lastOpened, now)}</span
              >
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .field-label {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .card {
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .row + .row {
    border-top: 1px solid var(--line);
  }

  .row {
    display: flex;
    align-items: center;
  }

  .entry {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 10px var(--s-4);
    border: 0;
    background: none;
    color: var(--text-1);
    text-align: left;
  }

  button.entry:hover {
    background: var(--bg-hover);
  }

  .entry.missing {
    color: var(--text-2);
  }

  .names {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .name {
    font-weight: 500;
  }

  .path,
  .mono {
    font-family: var(--font-mono);
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
    white-space: nowrap;
  }

  .when {
    width: 96px;
    text-align: right;
  }

  .warn {
    color: var(--warning-text);
  }

  .actions {
    display: flex;
    gap: var(--s-1);
    padding-right: var(--s-4);
  }
</style>
