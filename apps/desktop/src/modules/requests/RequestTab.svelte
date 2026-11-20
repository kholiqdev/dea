<!--
  A request's tab (F1-REQ-02): the request line, its sections, and what
  happened to its file outside Dea. The draft outlives the view, so
  switching tabs keeps the edits.
-->
<script lang="ts">
  import FileExclamationPoint from "@lucide/svelte/icons/file-exclamation-point";
  import FileX from "@lucide/svelte/icons/file-x";
  import { tick, untrack } from "svelte";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import Placeholder from "@dea/ui/Placeholder.svelte";
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import { errorMessage } from "$lib/ipc/errors";
  import BodySection from "./components/BodySection.svelte";
  import HeadersSection from "./components/HeadersSection.svelte";
  import OutsideChange from "./components/OutsideChange.svelte";
  import ParamsSection from "./components/ParamsSection.svelte";
  import RequestLine from "./components/RequestLine.svelte";
  import {
    draftFor,
    drafts,
    pruneDrafts,
    type Section,
  } from "./state/drafts.svelte";
  import { VariableLookup } from "./state/variables.svelte";

  let { tab }: { tab: { id: string; title: string; params: unknown } } =
    $props();

  const SECTIONS: { id: Section; label: string }[] = [
    { id: "params", label: "Params" },
    { id: "headers", label: "Headers" },
    { id: "body", label: "Body" },
    { id: "settings", label: "Settings" },
  ];

  const project = projectApi();
  const shell = shellApi();
  const uid = $props.id();
  // The shell makes the view anew for another tab, so the id stays.
  const id = untrack(() => tab.id);
  const path = $derived((tab.params as { path?: string } | null)?.path ?? "");
  const draft = untrack(() => draftFor(id, path));
  const variables = new VariableLookup(
    () => draft.inspection,
    () => project.environment?.name ?? null,
  );
  const readonly = $derived(project.info?.readOnly === true);
  const counts = $derived(draft.counts);
  let tablist = $state<HTMLElement>();

  shell.setSaveHandler(
    "requests.request",
    (tabId) => drafts.get(tabId)?.save() ?? Promise.resolve(),
  );

  // A rename or move from the sidebar gives the tab a new path.
  $effect(() => {
    const to = path;
    untrack(() => draft.moveTo(to));
  });

  // A change on disk to this request (the batch shown at first is old).
  let seen = project.changed;
  $effect(() => {
    const changed = project.changed;
    if (changed === seen) return;
    seen = changed;
    untrack(() => {
      if (changed.includes(draft.path)) void draft.check();
    });
  });

  // Inspects again after an edit, a move, a new environment, or any change
  // on disk, such as a folder's settings.
  $effect(() => {
    if (!draft.saved) return;
    void JSON.stringify(draft.draft);
    void draft.path;
    void project.environment;
    void project.changed;
    untrack(() => draft.scheduleInspection());
    return () => draft.cancelInspection();
  });

  $effect(() => shell.setTabDirty(id, draft.dirty));

  // The tab is named after the request.
  $effect(() => {
    const name = draft.saved?.name;
    if (name && name !== untrack(() => tab.title))
      shell.updateTab(id, { title: name });
  });

  // A closed tab takes its edits with it; reopening it reads the file.
  $effect(() => () => pruneDrafts());

  function save(): void {
    draft.save().catch((e: unknown) => shell.toast(errorMessage(e)));
  }

  function close(): void {
    shell.closeTab(id).catch((e: unknown) => shell.toast(errorMessage(e)));
  }

  async function onkeydown(e: KeyboardEvent): Promise<void> {
    const at = SECTIONS.findIndex((s) => s.id === draft.section);
    const to: Record<string, number> = {
      ArrowRight: (at + 1) % SECTIONS.length,
      ArrowLeft: (at - 1 + SECTIONS.length) % SECTIONS.length,
      Home: 0,
      End: SECTIONS.length - 1,
    };
    const next = SECTIONS[to[e.key] ?? -1];
    if (!next) return;
    e.preventDefault();
    draft.section = next.id;
    await tick();
    tablist?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
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

{#if draft.missing}
  <Placeholder title="This request no longer exists" icon={FileX} text="" />
{:else if !draft.saved}
  {#if draft.error}{@render problem(draft.error)}{/if}
{:else}
  <div class="editor">
    <OutsideChange {draft} onclose={close} />
    {#if draft.error}{@render problem(draft.error)}{/if}
    {#if draft.inspectionError}{@render problem(draft.inspectionError)}{/if}
    <RequestLine {draft} {variables} {readonly} onsave={save} />
    <div
      class="sections"
      role="tablist"
      aria-label="Request sections"
      tabindex="-1"
      bind:this={tablist}
      onkeydown={(e) => void onkeydown(e)}
    >
      {#each SECTIONS as section (section.id)}
        {@const selected = draft.section === section.id}
        <button
          type="button"
          role="tab"
          id="{uid}-{section.id}"
          aria-selected={selected}
          aria-controls="{uid}-panel"
          tabindex={selected ? 0 : -1}
          onclick={() => (draft.section = section.id)}
        >
          {section.label}
          {#if counts[section.id]}<span class="count">{counts[section.id]}</span
            >{/if}
        </button>
      {/each}
    </div>
    <div
      class="panel"
      role="tabpanel"
      id="{uid}-panel"
      aria-labelledby="{uid}-{draft.section}"
    >
      {#if draft.section === "params"}
        <ParamsSection {draft} {variables} {readonly} />
      {:else if draft.section === "headers"}
        <HeadersSection {draft} {variables} {readonly} />
      {:else if draft.section === "body"}
        <BodySection
          {draft}
          {variables}
          {readonly}
          onerror={(message: string) => shell.toast(message)}
        />
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

  .sections {
    height: 40px;
    flex: none;
    display: flex;
    gap: var(--s-5);
    padding: 0 var(--s-4);
    box-shadow:
      inset 0 1px 0 var(--line),
      inset 0 -1px 0 var(--line);
    overflow-x: auto;
  }

  .sections:focus {
    outline: none;
  }

  [role="tab"] {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    white-space: nowrap;
  }

  [role="tab"]:hover {
    color: var(--text-1);
  }

  [role="tab"][aria-selected="true"] {
    color: var(--text-1);
    box-shadow: inset 0 -2px 0 var(--text-1);
  }

  .count {
    font-family: var(--font-mono);
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
</style>
