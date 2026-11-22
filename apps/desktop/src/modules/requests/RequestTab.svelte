<!--
  A request's tab (F1-REQ-02): the request line, its sections, and what
  happened to its file outside Dea. The draft and its send outlive the
  view, so switching tabs keeps the edits and the response.
-->
<script lang="ts">
  import FileExclamationPoint from "@lucide/svelte/icons/file-exclamation-point";
  import FileX from "@lucide/svelte/icons/file-x";
  import { tick, untrack } from "svelte";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import Placeholder from "@dea/ui/Placeholder.svelte";
  import { projectApi } from "$app/contributions/project-api";
  import { shellApi } from "$app/contributions/shell-api";
  import type { ResponseLayout } from "$lib/ipc/bindings/ResponseLayout";
  import type { SendOptions } from "$lib/ipc/bindings/SendOptions";
  import { errorMessage } from "$lib/ipc/errors";
  import { settings } from "$lib/settings/settings.svelte";
  import BodySection from "./components/BodySection.svelte";
  import HeadersSection from "./components/HeadersSection.svelte";
  import NetworkOptions from "./components/NetworkOptions.svelte";
  import OutsideChange from "./components/OutsideChange.svelte";
  import ResponsePane from "./components/response/ResponsePane.svelte";
  import ParamsSection from "./components/ParamsSection.svelte";
  import RequestLine from "./components/RequestLine.svelte";
  import ResolvedUrl from "./components/ResolvedUrl.svelte";
  import SplitPane from "./components/SplitPane.svelte";
  import {
    draftFor,
    drafts,
    pruneDrafts,
    type Section,
  } from "./state/drafts.svelte";
  import TlsWarning from "./components/TlsWarning.svelte";
  import UndefinedWarning from "./components/UndefinedWarning.svelte";
  import { sendKeepingFocus } from "./lib/send-focus";
  import { whereUsed, type Usage } from "./lib/usage";
  import { exchangeFor } from "./state/exchange.svelte";
  import { DEFAULT_OPTIONS } from "./state/folders.svelte";
  import { setResponseSplit, toggleResponseLayout } from "./state/layout";
  import { saveTab } from "./state/synced-file.svelte";
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
  const exchange = exchangeFor(id);
  const variables = new VariableLookup(
    () => draft.inspection,
    () => project.environment?.name ?? null,
  );
  const readonly = $derived(project.info?.readOnly === true);
  const counts = $derived(draft.counts);
  // What the request will use: its own settings over what it inherits.
  const effective = $derived({
    ...(draft.inspection?.options ?? DEFAULT_OPTIONS),
    ...Object.fromEntries(
      Object.entries(draft.draft.options ?? {}).filter(
        ([, v]) => v !== undefined,
      ),
    ),
  });
  // Where the first undefined variable of the warning is used.
  const usage = $derived.by(() => {
    const name = exchange.warning?.[0];
    return name === undefined ? null : whereUsed(draft.draft, name);
  });
  // The split puts the response below when the work area is too narrow.
  let stacked = $state(false);
  let tablist = $state<HTMLElement>();
  let view = $state<HTMLElement>();

  shell.setSaveHandler("requests.request", (tabId) => saveTab(drafts, tabId));

  // A rename or move from the sidebar gives the tab a new path. A change on
  // disk is looked at by the module, as a tab not shown needs it too.
  $effect(() => {
    const to = path;
    untrack(() => draft.moveTo(to));
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

  // An edit takes the undefined-variable warning down: it was about the
  // draft as it was. The fields are listed one by one, so `name` is never
  // read: a rename from the sidebar rewrites it without an edit. The first
  // run only notes the draft, as the view is made anew on each tab switch
  // while the warning stays.
  let seen: string | null = null;
  $effect(() => {
    const d = draft.draft;
    const now = JSON.stringify([
      d.method,
      d.url,
      d.pathVariables,
      d.params,
      d.headers,
      d.body,
      d.auth,
      d.options,
    ]);
    const before = seen;
    seen = now;
    if (before !== null && before !== now)
      untrack(() => exchange.clearWarning());
  });

  $effect(() => shell.setTabDirty(id, draft.dirty));

  // Autosaves a second after the last edit, or after an outside change is
  // answered. Leaving the tab does not cancel it: the draft outlives the view.
  $effect(() => {
    void JSON.stringify(draft.draft);
    void draft.outside;
    void settings.requests.autosave;
    untrack(() => draft.scheduleAutosave());
  });

  // The tab is named after the request.
  $effect(() => {
    const name = draft.saved?.name;
    if (name && name !== untrack(() => tab.title))
      shell.updateTab(id, { title: name });
  });

  // The tab shows the method of the request. A tab restored from an older
  // session has only a path, and learns it here once the draft is loaded.
  $effect(() => {
    if (!draft.saved) return;
    const method = draft.draft.method;
    untrack(() => {
      const params = tab.params as { path: string; method?: string } | null;
      if (params && params.method !== method)
        shell.updateTab(id, { params: { ...params, method } });
    });
  });

  // A closed tab takes its edits with it; reopening it reads the file.
  $effect(() => () => pruneDrafts());

  function save(): void {
    draft.save().catch((e: unknown) => shell.toast(errorMessage(e)));
  }

  /** Sends; not confirmed, the focus goes back to where it was. */
  function send(opts: { anyway?: boolean } = {}): void {
    void sendKeepingFocus(exchange, draft, opts);
  }

  /** Calls `find` until it gives an element, which the editor's lazy load may delay. */
  async function focusWhenFound(find: () => HTMLElement | null | undefined) {
    for (let tries = 0; tries < 40; tries++) {
      const found = find();
      if (found) {
        found.focus();
        return;
      }
      await new Promise((done) => setTimeout(done, 50));
    }
  }

  /** Opens where the warning's first variable is used and focuses it. */
  async function showUsage(place: Usage): Promise<void> {
    if (place.section !== "url") draft.section = place.section;
    await tick();
    if (place.section === "url")
      await focusWhenFound(() =>
        view?.querySelector<HTMLElement>('input[aria-label="URL"]'),
      );
    else if (place.section === "body")
      await focusWhenFound(() =>
        view?.querySelector<HTMLElement>('.cm-content[aria-label="Body"]'),
      );
    else {
      const table = place.section === "params" ? "Query parameters" : "Headers";
      await focusWhenFound(() => {
        const rows = view
          ?.querySelector(`table[aria-label="${table}"]`)
          ?.querySelectorAll("tbody tr");
        return rows?.[place.row]?.querySelector<HTMLElement>("td.value input");
      });
    }
  }

  /** Saves the request panel's share; a refused save is a toast. */
  function resize(layout: ResponseLayout, percent: number): Promise<void> {
    return setResponseSplit(layout, percent).catch((e: unknown) => {
      shell.toast(errorMessage(e));
    });
  }

  function close(): void {
    shell.closeTab(id).catch((e: unknown) => shell.toast(errorMessage(e)));
  }

  /**
   * Escape stops the send when the focus is in this tab, or nowhere (a
   * clicked button may not take it), and nothing inside used the key.
   */
  function onwindowkeydown(e: KeyboardEvent): void {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    if (exchange.state.phase !== "sending") return;
    const target = e.target;
    if (
      target !== document.body &&
      !(target instanceof Node && view?.contains(target))
    )
      return;
    exchange.cancel();
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

<svelte:window onkeydown={onwindowkeydown} />

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
  <div class="view" bind:this={view}>
    <div class="head">
      <RequestLine
        {draft}
        {variables}
        {readonly}
        production={project.environment?.production ?? false}
        sending={exchange.state.phase === "sending" && exchange.state.started}
        onsave={save}
        onsend={() => send()}
        oncancel={() => exchange.cancel()}
      />
      <ResolvedUrl
        url={draft.inspection?.url ?? null}
        undefinedNames={draft.inspection?.undefined ?? []}
      />
    </div>
    <OutsideChange {draft} onclose={close} />
    {#if draft.error}{@render problem(draft.error)}{/if}
    {#if draft.inspectionError}{@render problem(draft.inspectionError)}{/if}
    {#if exchange.warning}
      <UndefinedWarning
        names={exchange.warning}
        environment={project.environment?.name ?? null}
        {usage}
        ondefine={() => void shell.runCommand("project.editEnvironments")}
        onshow={() => usage && void showUsage(usage)}
        onsendanyway={() => send({ anyway: true })}
      />
    {/if}
    <TlsWarning {effective} />
    <SplitPane
      layout={settings.requests.responseLayout}
      split={settings.requests.responseSplit}
      oncommit={resize}
      bind:stacked
    >
      {#snippet editor()}
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
              {#if counts[section.id]}<span class="count"
                  >{counts[section.id]}</span
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
            <ParamsSection
              {draft}
              {variables}
              {readonly}
              highlight={usage?.section === "params" ? usage.row : null}
            />
          {:else if draft.section === "headers"}
            <HeadersSection
              {draft}
              {variables}
              {readonly}
              highlight={usage?.section === "headers" ? usage.row : null}
            />
          {:else if draft.section === "body"}
            <BodySection
              {draft}
              {variables}
              {readonly}
              onerror={(message: string) => shell.toast(message)}
            />
          {:else}
            <NetworkOptions
              options={draft.draft.options ?? {}}
              effective={draft.inspection?.options ?? null}
              scope="request"
              disabled={readonly}
              onchange={(options: SendOptions) => draft.setOptions(options)}
            />
          {/if}
        </div>
      {/snippet}
      {#snippet response()}
        <ResponsePane
          tabId={id}
          name={tab.title}
          {exchange}
          layout={settings.requests.responseLayout}
          {stacked}
          ontogglelayout={() => void toggleResponseLayout()}
        />
      {/snippet}
    </SplitPane>
  </div>
{/if}

<style>
  .view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* The request bar and the URL under it span the tab. */
  .head {
    flex: none;
    display: flex;
    flex-direction: column;
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .sections {
    height: 36px;
    flex: none;
    display: flex;
    gap: var(--s-5);
    padding: 0 var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
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
