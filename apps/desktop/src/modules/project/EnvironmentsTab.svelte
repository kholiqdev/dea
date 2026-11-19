<!-- The environment editor (F1-FON-06), one tab per window. -->
<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Button from "@dea/ui/Button.svelte";
  import { environmentColor, projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { errorMessage } from "$lib/ipc/errors";
  import DeleteEnvironment from "./components/DeleteEnvironment.svelte";
  import EnvironmentInspector from "./components/EnvironmentInspector.svelte";
  import VariablesTable from "./components/VariablesTable.svelte";
  import {
    environmentsEditor,
    type EnvironmentDraft,
  } from "./state/environments.svelte";

  let { tab }: { tab: { id: string } } = $props();

  const project = projectApi();
  const editor = environmentsEditor();
  const selected = $derived(editor.selected);
  const problems = $derived(selected ? editor.problems(selected) : []);
  const readonly = $derived(project.info?.readOnly === true);
  let deleting = $state<EnvironmentDraft | null>(null);

  // The tab bar shows unsaved changes, and a close asks to save them.
  $effect(() => shellApi().setTabDirty(tab.id, editor.dirty));
  shellApi().setSaveHandler("project.environments", () => editor.save());

  function attempt(action: () => Promise<unknown>): void {
    action().catch((e: unknown) => (editor.error = errorMessage(e)));
  }

  function focusName(): void {
    document
      .querySelector<HTMLInputElement>('.editor input[aria-label="Name"]')
      ?.focus();
  }
</script>

<div class="editor">
  <nav class="list" aria-label="Environments">
    <div class="list-head">
      <h2>Environments</h2>
      {#if !readonly}
        <button
          type="button"
          class="iconbtn"
          aria-label="New environment"
          title="New environment"
          onclick={() => editor.newEnvironment()}
        >
          <Plus size={16} strokeWidth={1.5} />
        </button>
      {/if}
    </div>
    {#each editor.drafts as d (d)}
      <button
        type="button"
        class="srow"
        aria-current={d === selected ? "true" : undefined}
        onclick={() => (editor.selected = d)}
      >
        <span class="dot" style:background={environmentColor(d.draft)}></span>
        <span class="label">{d.draft.name}</span>
        {#if d.draft.production}<span class="prod">production</span>{/if}
        {#if d.dirty}<span class="dirty" role="img" aria-label="Unsaved changes"
          ></span>{/if}
        <span class="count">{d.draft.variables?.length ?? 0}</span>
      </button>
    {/each}
    <span class="foot"
      >{editor.drafts.length} environments in {project.info?.name}</span
    >
  </nav>

  {#if selected}
    <main class="main">
      <div class="toolbar">
        <div class="heading">
          <h1>{selected.draft.name}</h1>
          <span class="meta">
            {selected.draft.variables?.length ?? 0} variables. A value is plain text
            or a Vault reference like {"{{vault:name}}"}
          </span>
        </div>
        <div class="end">
          {#if !readonly}
            <Button
              variant="ghost"
              disabled={!selected.saved}
              onclick={() => attempt(() => editor.duplicate(selected))}
            >
              <Copy size={16} strokeWidth={1.5} />Duplicate
            </Button>
            <Button variant="ghost" onclick={focusName}>
              <Pencil size={16} strokeWidth={1.5} />Rename
            </Button>
            <Button
              variant="primary"
              disabled={!editor.canSave}
              onclick={() => attempt(() => editor.save())}
            >
              Save
            </Button>
          {/if}
        </div>
      </div>
      {#if problems.length > 0}
        <ul class="problems" role="alert">
          {#each problems as p (p)}<li>{p}</li>{/each}
        </ul>
      {/if}
      {#if editor.error}<p class="error">{editor.error}</p>{/if}
      <div class="work">
        <section class="pane" aria-label="Variables">
          <VariablesTable
            draft={selected}
            secretNames={project.info?.secretNames ?? []}
            {readonly}
          />
        </section>
        <EnvironmentInspector
          draft={selected}
          {readonly}
          ondelete={() => (deleting = selected)}
        />
      </div>
    </main>
  {/if}
</div>

{#if deleting}
  <DeleteEnvironment
    name={deleting.draft.name}
    oncancel={() => (deleting = null)}
    ondelete={() => {
      // Take the draft before clearing it: the dialog goes away with it.
      const target = deleting;
      deleting = null;
      if (target) attempt(() => editor.remove(target));
    }}
  />
{/if}

<style>
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .list {
    width: 240px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 var(--s-2) var(--s-4);
    background: var(--bg-0);
    border-right: 1px solid var(--line);
  }

  .list-head {
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0 0 var(--s-2);
  }

  h2 {
    font-size: 13px;
    font-weight: 500;
  }

  .srow {
    height: 32px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-ctl);
    background: none;
    color: var(--text-2);
    text-align: left;
  }

  .srow:hover {
    background: var(--bg-hover);
  }

  .srow[aria-current="true"] {
    background: var(--bg-3);
    color: var(--text-1);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 4px;
  }

  .prod {
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 400 11px/1 var(--font-mono);
    color: var(--negative-text);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .dirty {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: var(--text-2);
  }

  .count,
  .foot,
  .meta {
    font-size: 12px;
    color: var(--text-3);
  }

  .count {
    font-family: var(--font-mono);
  }

  .foot {
    margin-top: auto;
    padding: var(--s-2);
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .heading {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  h1 {
    font-size: 15px;
    line-height: 20px;
    font-weight: 500;
  }

  .end {
    margin-left: auto;
    display: flex;
    gap: var(--s-2);
  }

  .problems,
  .error {
    margin: var(--s-2) var(--s-4) 0;
    color: var(--negative-text);
  }

  .work {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .pane {
    flex: 1;
    min-width: 0;
    overflow: auto;
  }

  .iconbtn {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-3);
  }

  .iconbtn:hover {
    color: var(--text-1);
    background: var(--bg-hover);
  }
</style>
