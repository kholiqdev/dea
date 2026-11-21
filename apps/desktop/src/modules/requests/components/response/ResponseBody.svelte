<!--
  The body of a response in the view its kind offers: Pretty and Raw in the
  read-only editor, a JSONPath over JSON, and a word for what has no text.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { BodyText } from "$lib/ipc/bindings/BodyText";
  import type { Filtered } from "$lib/ipc/bindings/Filtered";
  import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
  import { errorMessage } from "$lib/ipc/errors";
  import { responseText } from "$lib/ipc/responses";
  import { formatBytes } from "../../lib/format";
  import type { BodyView } from "./body-view";
  import JsonFilter from "./JsonFilter.svelte";
  import ViewerPlaceholder from "./ViewerPlaceholder.svelte";

  let {
    tabId,
    response,
    view,
    onview,
  }: {
    tabId: string;
    response: ResponseInfo;
    /** The view chosen for this response; `null` for the first of its kind. */
    view: BodyView | null;
    onview: (view: BodyView) => void;
  } = $props();

  const LABELS: Record<BodyView, string> = {
    pretty: "Pretty",
    raw: "Raw",
    preview: "Preview",
  };
  const KINDS: Record<string, string> = {
    json: "JSON",
    xml: "XML",
    html: "HTML",
  };

  const kind = $derived(response.kind);
  const viewable = $derived(
    kind === "json" || kind === "xml" || kind === "html" || kind === "text",
  );
  const large = $derived(viewable && response.large);
  const views = $derived<BodyView[]>(
    kind === "json" || kind === "xml"
      ? ["pretty", "raw"]
      : kind === "html"
        ? ["pretty", "raw", "preview"]
        : kind === "image" || kind === "pdf"
          ? ["preview"]
          : kind === "text"
            ? ["raw"]
            : [],
  );
  const current = $derived(
    view !== null && views.includes(view) ? view : (views[0] ?? "raw"),
  );
  const language = $derived(
    kind === "json" || kind === "xml" || kind === "html" ? kind : "text",
  );
  const filterable = $derived(kind === "json" && !response.large);

  let loaded = $state<{
    key: string;
    body: BodyText | null;
    error?: string;
  } | null>(null);
  let filtered = $state<Filtered | null>(null);
  const wanted = $derived(`${tabId}:${current}`);

  $effect(() => {
    const key = wanted;
    if (!viewable || large || current === "preview") return;
    // A new response or view is asked for again.
    void response;
    let live = true;
    responseText(tabId, current)
      .then((body) => {
        if (live) loaded = { key, body };
      })
      .catch((e: unknown) => {
        if (live) loaded = { key, body: null, error: errorMessage(e) };
      });
    return () => (live = false);
  });

  const body = $derived(loaded?.key === wanted ? loaded.body : null);
  const unread = $derived(
    loaded?.key === wanted ? (loaded.error ?? null) : null,
  );

  let radios = $state<HTMLElement>();
  async function onviewkeys(e: KeyboardEvent): Promise<void> {
    const at = views.indexOf(current);
    const to: Record<string, number> = {
      ArrowRight: (at + 1) % views.length,
      ArrowDown: (at + 1) % views.length,
      ArrowLeft: (at - 1 + views.length) % views.length,
      ArrowUp: (at - 1 + views.length) % views.length,
      Home: 0,
      End: views.length - 1,
    };
    const next = views[to[e.key] ?? -1];
    if (!next) return;
    e.preventDefault();
    onview(next);
    await tick();
    radios?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
  }
  const shown = $derived(
    filterable && current !== "preview" && filtered
      ? filtered.text
      : body?.text,
  );
  const folded = $derived(current === "pretty" || filtered !== null);
</script>

<div class="body">
  {#if kind === "empty"}
    <p class="note">No body</p>
  {:else if kind === "binary"}
    <p class="note">
      Binary body, {formatBytes(response.size)}. Save it to a file to open it.
    </p>
  {:else if kind === "events"}
    <ViewerPlaceholder slot="event-list" />
  {:else if large}
    <ViewerPlaceholder slot="large-body" />
  {:else}
    <div
      class="views"
      role="radiogroup"
      aria-label="Body view"
      tabindex="-1"
      bind:this={radios}
      onkeydown={(e) => void onviewkeys(e)}
    >
      {#each views as option (option)}
        <button
          type="button"
          role="radio"
          aria-checked={current === option}
          tabindex={current === option ? 0 : -1}
          onclick={() => onview(option)}
        >
          {LABELS[option]}
        </button>
      {/each}
    </div>
    {#if current === "preview"}
      <ViewerPlaceholder slot="preview" />
    {:else}
      {#if filterable}
        <JsonFilter
          {tabId}
          onresult={(result: Filtered | null) => (filtered = result)}
        />
      {/if}
      {#if current === "pretty" && body && !body.formatted && !filtered}
        <p class="unformatted">
          Not valid {KINDS[kind] ?? "text"}, shown as received.
        </p>
      {/if}
      {#if unread !== null}
        <p class="unread" role="alert">The body could not be read: {unread}</p>
      {/if}
      <div class="editor">
        {#if shown !== undefined}
          {#key `${current}:${folded}`}
            {#await import("@dea/editor/CodeEditor.svelte") then { default: CodeEditor }}
              <CodeEditor
                value={shown}
                language={filtered
                  ? "json"
                  : current === "pretty"
                    ? language
                    : "text"}
                label="Response body"
                readonly
                fold={folded}
                search
              />
            {:catch}
              <p class="load-error">The editor could not load.</p>
            {/await}
          {/key}
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .note,
  .unformatted {
    margin: 0;
    color: var(--text-3);
  }

  .note {
    padding: var(--s-4);
  }

  .unformatted {
    flex: none;
    padding: var(--s-2) var(--s-4);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .views {
    flex: none;
    display: inline-flex;
    align-self: flex-start;
    margin: var(--s-2) var(--s-4);
    padding: 2px;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .views button {
    height: 24px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .views button[aria-checked="true"] {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .unread {
    margin: 0;
    padding: var(--s-2) var(--s-4);
    color: var(--negative-text);
  }

  .views:focus {
    outline: none;
  }

  .editor {
    flex: 1;
    min-height: 0;
  }

  .load-error {
    margin: 0;
    padding: var(--s-4);
    color: var(--negative-text);
  }
</style>
