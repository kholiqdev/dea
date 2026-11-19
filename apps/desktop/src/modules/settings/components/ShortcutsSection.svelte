<script lang="ts">
  import Search from "@lucide/svelte/icons/search";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import {
    ShortcutEditor,
    resetAllShortcuts,
    resetShortcut,
    shortcutRows,
  } from "../state/shortcuts.svelte";
  import ShortcutCapture from "./ShortcutCapture.svelte";

  const platform = currentPlatform();
  const editor = new ShortcutEditor();
  let query = $state("");
  let error = $state<string | null>(null);
  let confirmReset = $state(false);

  const rows = $derived(shortcutRows());
  const shown = $derived(
    rows.filter((r) =>
      r.title.toLowerCase().includes(query.trim().toLowerCase()),
    ),
  );
  const titleOf = (command: string) =>
    rows.find((r) => r.command === command)?.title ?? command;

  async function run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

{#snippet keys(chord: string)}
  <span class="keys">
    {#each chordParts(parseChord(chord), platform) as part, i (i)}
      <kbd class="kc">{part}</kbd>
    {/each}
  </span>
{/snippet}

<div class="page">
  <div class="page-head">
    <h1>Keyboard shortcuts</h1>
    <p>Every shortcut can be changed. A clash is shown before it is saved.</p>
  </div>

  <label class="filter">
    <Search size={16} strokeWidth={1.5} />
    <input
      type="search"
      placeholder="Search shortcuts"
      aria-label="Search shortcuts"
      bind:value={query}
    />
  </label>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <div class="card">
    <table class="table" aria-label="Shortcuts">
      <thead>
        <tr>
          <th scope="col">Action</th>
          <th scope="col">Shortcut</th>
          <th scope="col"><span class="sr">Change</span></th>
        </tr>
      </thead>
      <tbody>
        {#each shown as row (row.command)}
          {@const editing = editor.command === row.command}
          <tr aria-selected={editing}>
            <td class="strong">{row.title}</td>
            <td>
              {#if editing}
                <span class="edit">
                  <ShortcutCapture
                    label="New shortcut for {row.title}"
                    value={editor.captured}
                    oncapture={(chord: string) => editor.capture(chord)}
                    oncancel={() => editor.cancel()}
                  />
                  {#if editor.clashes.length > 0}
                    <span class="clash">
                      <TriangleAlert size={14} strokeWidth={1.5} />
                      Already used by {editor.clashes.map(titleOf).join(", ")}
                    </span>
                  {/if}
                </span>
              {:else if row.chord}
                {@render keys(row.chord)}
              {:else}
                <span class="off">Off</span>
              {/if}
            </td>
            <td class="actions">
              <div class="buttons">
                {#if editing}
                  <Button
                    size="small"
                    variant="primary"
                    disabled={!editor.canSave}
                    onclick={() => run(() => editor.save())}>Save</Button
                  >
                  {#if editor.clashes.length > 0}
                    <Button
                      size="small"
                      onclick={() => run(() => editor.useHere())}
                      >Use it here instead</Button
                    >
                  {/if}
                  <Button
                    size="small"
                    variant="ghost"
                    onclick={() => run(() => editor.turnOff())}>Turn off</Button
                  >
                  <Button
                    size="small"
                    variant="ghost"
                    onclick={() => editor.cancel()}>Cancel</Button
                  >
                {:else}
                  {#if row.overridden}
                    <Button
                      size="small"
                      variant="ghost"
                      onclick={() => run(() => resetShortcut(row.command))}
                      >Reset to default</Button
                    >
                  {/if}
                  <Button
                    size="small"
                    variant="ghost"
                    onclick={() => editor.start(row.command)}>Change</Button
                  >
                {/if}
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  <div>
    <Button size="small" variant="danger" onclick={() => (confirmReset = true)}
      >Reset all to defaults…</Button
    >
  </div>
</div>

{#if confirmReset}
  <Dialog title="Reset every shortcut?" oncancel={() => (confirmReset = false)}>
    <p class="text">Every shortcut goes back to the one Dea comes with.</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirmReset = false)}
        >Cancel</Button
      >
      <Button
        variant="danger"
        onclick={() =>
          run(async () => {
            await resetAllShortcuts();
            confirmReset = false;
          })}>Reset all</Button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .page {
    max-width: 1080px;
    padding: var(--s-8);
    display: flex;
    flex-direction: column;
    gap: var(--s-6);
  }

  .page-head {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .page-head h1 {
    font-size: 24px;
    line-height: 32px;
    font-weight: 500;
  }

  .page-head p,
  .text {
    max-width: 60ch;
    color: var(--text-2);
  }

  .filter {
    max-width: 360px;
    height: 32px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 6px 0 var(--s-2);
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-3);
  }

  .filter input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: none;
    outline: none;
    color: var(--text-1);
  }

  .card {
    overflow-x: auto;
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
  }

  .table th,
  .table td {
    height: 40px;
    padding: 0 var(--s-3);
    text-align: left;
    white-space: nowrap;
    border-bottom: 1px solid var(--line);
  }

  .table th:first-child,
  .table td:first-child {
    padding-left: var(--s-4);
  }

  .table th {
    height: 32px;
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
  }

  .table td {
    color: var(--text-2);
  }

  .table td.strong {
    color: var(--text-1);
  }

  .table tr[aria-selected="true"] td {
    background: var(--bg-hover);
  }

  .table td.actions {
    white-space: normal;
    padding-top: var(--s-1);
    padding-bottom: var(--s-1);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--s-1);
  }

  .keys {
    display: inline-flex;
    gap: 2px;
  }

  .kc {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-2);
  }

  .edit {
    display: inline-flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--s-1);
    padding: var(--s-1) 0;
  }

  .clash {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--warning-text);
  }

  .off {
    color: var(--text-3);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .error {
    color: var(--negative-text);
  }
</style>
