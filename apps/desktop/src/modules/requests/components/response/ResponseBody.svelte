<!--
  The body of a response in the view its controls chose: Pretty and Raw in
  the read-only editor, what a JSONPath selects of JSON, and a word for what
  has no text. The view and the filter are set in the response header.
-->
<script lang="ts">
  import type { BodyText } from "$lib/ipc/bindings/BodyText";
  import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
  import { errorMessage } from "$lib/ipc/errors";
  import { responseText } from "$lib/ipc/responses";
  import { formatBytes } from "../../lib/format";
  import type { SseRow } from "../../state/exchange.svelte";
  import type { BodyControls } from "./body-controls.svelte";
  import EventList from "./EventList.svelte";
  import LargeBody from "./LargeBody.svelte";
  import Preview from "./Preview.svelte";

  let {
    tabId,
    name,
    response,
    events,
    dropped,
    startedAt,
    controls,
  }: {
    tabId: string;
    /** The request's name, which a saved body is named after. */
    name: string;
    response: ResponseInfo;
    /** The events of a stream, timed from `startedAt`. */
    events: SseRow[];
    dropped: number;
    startedAt: number;
    /** The view and the filter of this response. */
    controls: BodyControls;
  } = $props();

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
  const current = $derived(controls.view);
  const language = $derived(
    kind === "json" || kind === "xml" || kind === "html" ? kind : "text",
  );
  const filtered = $derived(controls.filtered);

  let loaded = $state<{
    key: string;
    body: BodyText | null;
    error?: string;
  } | null>(null);
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

  const shown = $derived(filtered ? filtered.text : body?.text);
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
    <EventList {events} {startedAt} {dropped} />
  {:else if large}
    <LargeBody
      id={tabId}
      size={response.size}
      {name}
      {kind}
      contentType={response.contentType}
    />
  {:else if current === "preview"}
    <Preview id={tabId} {kind} contentType={response.contentType} />
  {:else}
    {#if controls.filterError !== null}
      <p class="filter-error" role="alert">{controls.filterError}</p>
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

  .unread,
  .filter-error {
    margin: 0;
    padding: var(--s-2) var(--s-4);
    color: var(--negative-text);
  }

  .filter-error {
    flex: none;
    box-shadow: inset 0 -1px 0 var(--line);
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
