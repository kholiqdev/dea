<!--
  A folder's or collection's settings (F1-REQ-06): what its requests inherit.
  Like a request, it keeps its edits while another tab is shown.
-->
<script lang="ts">
  import FileExclamationPoint from "@lucide/svelte/icons/file-exclamation-point";
  import FileX from "@lucide/svelte/icons/file-x";
  import { untrack } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import Placeholder from "@dea/ui/Placeholder.svelte";
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
  import type { SendOptions } from "$lib/ipc/bindings/SendOptions";
  import type { TlsSettings } from "$lib/ipc/bindings/TlsSettings";
  import { errorMessage } from "$lib/ipc/errors";
  import CertificateOptions from "./components/CertificateOptions.svelte";
  import KeyValueTable from "./components/KeyValueTable.svelte";
  import NetworkOptions from "./components/NetworkOptions.svelte";
  import OutsideChange from "./components/OutsideChange.svelte";
  import {
    folderStateFor,
    folderStates,
    pruneFolderStates,
  } from "./state/folders.svelte";
  import { treeState } from "./state/tree.svelte";
  import { VariableLookup } from "./state/variables.svelte";

  let { tab }: { tab: { id: string; title: string; params: unknown } } =
    $props();

  const project = projectApi();
  const shell = shellApi();
  // The shell makes the view anew for another tab, so the id stays.
  const id = untrack(() => tab.id);
  const path = $derived((tab.params as { path?: string } | null)?.path ?? "");
  const state = untrack(() => folderStateFor(id, path));
  // A folder has no request to inspect, so no value is known yet.
  const variables = new VariableLookup(
    () => null,
    () => project.environment?.name ?? null,
  );
  const readonly = $derived(project.info?.readOnly === true);
  const collection = $derived(state.isCollection);
  const count = $derived.by(() => {
    const node = treeState.find(state.path);
    return node?.kind === "folder" ? node.requestCount : null;
  });
  const unit = $derived(collection ? "collection" : "folder");

  shell.setSaveHandler(
    "requests.folder",
    (tabId) => folderStates.get(tabId)?.save() ?? Promise.resolve(),
  );

  // A rename or move from the sidebar gives the tab a new path.
  $effect(() => {
    const to = path;
    untrack(() => state.moveTo(to));
  });

  // A change on disk to this folder, or to one above it that it inherits from.
  let seen = project.changed;
  $effect(() => {
    const changed = project.changed;
    if (changed === seen) return;
    seen = changed;
    untrack(() => {
      const own = `${state.path}/_folder.yaml`;
      if (changed.some((p) => p === own || p === state.path))
        void state.check();
      if (changed.some((p) => p.endsWith("/_folder.yaml")))
        void state.loadInherited();
    });
  });

  $effect(() => shell.setTabDirty(id, state.dirty));

  // A closed tab takes its edits with it; reopening it reads the file.
  $effect(() => () => pruneFolderStates());

  function save(): void {
    state.save().catch((e: unknown) => shell.toast(errorMessage(e)));
  }

  function close(): void {
    shell.closeTab(id).catch((e: unknown) => shell.toast(errorMessage(e)));
  }
</script>

{#snippet problem(text: string)}
  <FileBanner tone="negative">
    {#snippet icon()}<FileExclamationPoint
        size={16}
        strokeWidth={1.5}
      />{/snippet}
    {text}
  </FileBanner>
{/snippet}

{#if state.missing}
  <Placeholder title="This folder no longer exists" icon={FileX} text="" />
{:else if !state.saved}
  {#if state.error}{@render problem(state.error)}{/if}
{:else}
  <div class="editor">
    <OutsideChange draft={state} noun={unit} onclose={close} />
    {#if state.error}{@render problem(state.error)}{/if}
    <header class="toolbar">
      <div class="heading">
        <h1>{tab.title}</h1>
        <p class="hint">
          These settings are inherited by the {count === null
            ? ""
            : `${count} `}{count === 1 ? "request" : "requests"} in this {unit}.
          A request can override any of them.
        </p>
      </div>
      <Button variant="primary" disabled={readonly} onclick={save}>Save</Button>
    </header>
    <div class="panel">
      <section>
        <h2>Headers</h2>
        <KeyValueTable
          rows={state.draft.headers ?? []}
          label="Headers"
          keyLabel="Header"
          addLabel="Add header"
          {variables}
          disabled={readonly}
          onchange={(rows: KeyValue[]) => state.setHeaders(rows)}
        />
      </section>
      <section>
        <h2>Variables</h2>
        <KeyValueTable
          rows={state.draft.variables ?? []}
          label="Variables"
          keyLabel="Name"
          addLabel="Add variable"
          {variables}
          disabled={readonly}
          onchange={(rows: KeyValue[]) => state.setVariables(rows)}
        />
      </section>
      <section>
        <h2>Network</h2>
        <NetworkOptions
          options={state.draft.options ?? {}}
          effective={state.inherited}
          scope="folder"
          disabled={readonly}
          onchange={(options: SendOptions) => state.setOptions(options)}
        />
      </section>
      {#if collection}
        <section>
          <h2>Certificates</h2>
          <CertificateOptions
            tls={state.draft.tls}
            disabled={readonly}
            onerror={(message: string) => shell.toast(message)}
            onchange={(tls: TlsSettings) => state.setTls(tls)}
          />
        </section>
      {/if}
    </div>
  </div>
{/if}

<style>
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-4);
    padding: var(--s-3) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  h1 {
    font-size: 16px;
    line-height: 24px;
    font-weight: 500;
    color: var(--text-1);
  }

  .hint {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-6);
    padding: var(--s-4);
    overflow: auto;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  h2 {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }
</style>
