<!-- The environment editor (F1-FON-06), one tab per window. -->
<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import FileExclamationPoint from "@lucide/svelte/icons/file-exclamation-point";
  import FileX from "@lucide/svelte/icons/file-x";
  import Lock from "@lucide/svelte/icons/lock";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import { untrack } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import { environmentColor, projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { errorMessage } from "$lib/ipc/errors";
  import DeleteEnvironment from "./components/DeleteEnvironment.svelte";
  import EnvironmentInspector from "./components/EnvironmentInspector.svelte";
  import ProjectProblems from "./components/ProjectProblems.svelte";
  import VariablesTable from "./components/VariablesTable.svelte";
  import {
    differences,
    environmentsEditor,
    type EnvironmentDraft,
    type Outside,
  } from "./state/environments.svelte";

  let { tab }: { tab: { id: string } } = $props();

  const project = projectApi();
  const editor = environmentsEditor();
  const selected = $derived(editor.selected);
  const problems = $derived(selected ? editor.problems(selected) : []);
  const readonly = $derived(project.info?.readOnly === true);
  // The window's banner names a missing folder or a broken dea.yaml.
  const fileErrors = $derived(
    (project.info?.errors ?? []).filter((e) => e.path !== "dea.yaml"),
  );
  const newerDea = $derived(
    readonly &&
      project.info?.missing === false &&
      fileErrors.length === (project.info?.errors.length ?? 0),
  );
  let deleting = $state<EnvironmentDraft | null>(null);
  let comparing = $state<EnvironmentDraft | null>(null);
  const compare = $derived(
    comparing === selected && selected?.outside?.kind === "changed"
      ? differences(selected.draft, selected.outside.theirs)
      : null,
  );

  // Changes from disk arrive as a new ProjectInfo, one per watcher batch.
  $effect(() => {
    const info = project.info;
    if (info) untrack(() => editor.sync(info));
  });

  // The tab bar shows unsaved changes, and a close asks to save them.
  $effect(() => shellApi().setTabDirty(tab.id, editor.dirty));
  shellApi().setSaveHandler("project.environments", async () => {
    await editor.save();
    // A cancelled sign-in saves nothing; the tab must not close over that.
    if (editor.dirty) throw new Error("The environments were not saved");
  });

  function attempt(action: () => Promise<unknown>): void {
    action().catch((e: unknown) => (editor.error = errorMessage(e)));
  }

  function tag(outside: Outside | null): string | null {
    if (!outside) return null;
    if (outside.kind === "changed") return "changed on disk";
    if (outside.kind === "deleted") return "deleted";
    return outside.error.mergeConflict ? "merge conflict" : "invalid file";
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
        {#if tag(d.outside)}<span class="tag">{tag(d.outside)}</span>{/if}
        {#if d.dirty}<span class="dirty" role="img" aria-label="Unsaved changes"
          ></span>{/if}
        <span class="count">{d.draft.variables?.length ?? 0}</span>
      </button>
    {/each}
    <span class="foot"
      >{editor.drafts.length} environments in {project.info?.name}</span
    >
  </nav>

  <div class="main">
    {#if newerDea}
      <FileBanner tone="neutral">
        {#snippet icon()}<Lock size={16} strokeWidth={1.5} />{/snippet}
        This project was saved by a newer Dea. It is open read-only so nothing is
        lost.
      </FileBanner>
    {/if}
    {#if selected?.outside?.kind === "changed"}
      <FileBanner
        tone="warning"
        actions={[
          {
            label: "Compare",
            onclick: () =>
              (comparing = comparing === selected ? null : selected),
          },
          { label: "Keep mine", onclick: () => selected.keepMine() },
          { label: "Take theirs", onclick: () => selected.takeTheirs() },
        ]}
      >
        {#snippet icon()}<FileExclamationPoint
            size={16}
            strokeWidth={1.5}
          />{/snippet}
        <code>{selected.file}</code> changed on disk while you were editing it.
      </FileBanner>
    {:else if selected?.outside?.kind === "deleted"}
      <FileBanner
        tone="warning"
        actions={[
          { label: "Save it again", onclick: () => selected.saveAgain() },
          {
            label: "Discard changes",
            variant: "ghost",
            onclick: () => editor.discard(selected),
          },
        ]}
      >
        {#snippet icon()}<FileX size={16} strokeWidth={1.5} />{/snippet}
        <code>{selected.file}</code> was deleted outside Dea. Your changes to it are
        still here.
      </FileBanner>
    {/if}
    {#if compare}
      <section class="compare" aria-label="Compare">
        <h3>Compare</h3>
        <div class="sides">
          <div class="side" role="group" aria-label="Mine, not saved">
            <span class="side-label">Mine, not saved</span>
            <div class="code">
              {#each compare as row (row.field)}
                <div>{row.field}: {row.mine ?? "(none)"}</div>
              {/each}
            </div>
          </div>
          <div class="side" role="group" aria-label="On disk">
            <span class="side-label">On disk</span>
            <div class="code">
              {#each compare as row (row.field)}
                <div>{row.field}: {row.theirs ?? "(none)"}</div>
              {/each}
            </div>
          </div>
        </div>
      </section>
    {/if}
    <ProjectProblems errors={fileErrors} />
    {#if selected}
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
    {/if}
  </div>
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

  .tag {
    font: 400 12px/16px var(--font-mono);
    color: var(--text-3);
    white-space: nowrap;
  }

  .compare {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    padding: var(--s-4) var(--s-4) 0;
  }

  h3,
  .side-label {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .side-label {
    color: var(--text-3);
  }

  .sides {
    display: flex;
    gap: var(--s-3);
  }

  .side {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .code {
    padding: var(--s-3) var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 400 12px/20px var(--font-mono);
    color: var(--text-2);
    overflow-wrap: anywhere;
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
