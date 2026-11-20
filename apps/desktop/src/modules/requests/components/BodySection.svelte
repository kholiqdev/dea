<!--
  The request's body (F1-REQ-02 #7): a mode, and what that mode needs. The
  draft keeps what each mode held while the tab is open; the file keeps only
  the chosen mode. Content-Type comes from the inspection's automatic
  headers, so nothing here sets it.
-->
<script lang="ts">
  import { formatJson, lintJson } from "@dea/editor";
  import type { Body } from "$lib/ipc/bindings/Body";
  import type { FormField } from "$lib/ipc/bindings/FormField";
  import type { KeyValue } from "$lib/ipc/bindings/KeyValue";
  import type { RawLanguage } from "$lib/ipc/bindings/RawLanguage";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import type { BodyMode, RequestDraft } from "../state/drafts.svelte";
  import type { VariableLookup } from "../state/variables.svelte";
  import FileChoice from "./FileChoice.svelte";
  import FormDataTable from "./FormDataTable.svelte";
  import KeyValueTable, { sendLabel } from "./KeyValueTable.svelte";

  let {
    draft,
    variables,
    readonly,
    onerror,
  }: {
    draft: RequestDraft;
    variables: VariableLookup;
    readonly: boolean;
    onerror?: (message: string) => void;
  } = $props();

  const MODES: { id: BodyMode; label: string }[] = [
    { id: "none", label: "None" },
    { id: "json", label: "JSON" },
    { id: "formData", label: "Form data" },
    { id: "urlencoded", label: "URL encoded" },
    { id: "raw", label: "Raw" },
    { id: "binary", label: "Binary" },
  ];

  const LANGUAGES: { id: RawLanguage; label: string }[] = [
    { id: "text", label: "Text" },
    { id: "xml", label: "XML" },
    { id: "html", label: "HTML" },
    { id: "javascript", label: "JavaScript" },
  ];

  const uid = $props.id();
  // The code editor's own Format key.
  const formatKey = formatChordText("Mod+Shift+F", currentPlatform());
  const body = $derived(draft.draft.body);
  const text = $derived(
    body?.mode === "json" || body?.mode === "raw" ? body.text : "",
  );
  const canFormat = $derived(
    !readonly &&
      body?.mode === "json" &&
      text.trim() !== "" &&
      lintJson(text) === null,
  );

  function setText(next: string): void {
    if (body?.mode === "json") draft.setBody({ ...body, text: next });
    else if (body?.mode === "raw") draft.setBody({ ...body, text: next });
  }

  function format(): void {
    const formatted = formatJson(text);
    if (formatted !== null) setText(formatted);
  }

  const asBody = (b: Body) => draft.setBody(b);
</script>

<div class="top">
  <div class="modes" role="radiogroup" aria-label="Body type">
    {#each MODES as mode (mode.id)}
      <label class:on={draft.bodyMode === mode.id}>
        <input
          type="radio"
          name="{uid}-mode"
          value={mode.id}
          checked={draft.bodyMode === mode.id}
          disabled={readonly}
          onchange={() => draft.setBodyMode(mode.id)}
        />{mode.label}
      </label>
    {/each}
  </div>
  {#if body?.mode === "raw"}
    <select
      aria-label="Raw language"
      value={body.language}
      disabled={readonly}
      onchange={(e) =>
        asBody({
          ...body,
          language: e.currentTarget.value as RawLanguage,
        })}
    >
      {#each LANGUAGES as language (language.id)}
        <option value={language.id}>{language.label}</option>
      {/each}
    </select>
  {/if}
  {#if body?.mode === "json"}
    <button type="button" class="format" disabled={!canFormat} onclick={format}>
      Format <kbd class="kc">{formatKey}</kbd>
    </button>
  {/if}
</div>

{#if body?.mode === "json" || body?.mode === "raw"}
  <div class="editor">
    {#await import("@dea/editor/CodeEditor.svelte") then { default: CodeEditor }}
      <CodeEditor
        value={text}
        language={body.mode === "json" ? "json" : body.language}
        label="Body"
        {readonly}
        {variables}
        onchange={setText}
      />
    {:catch}
      <p class="load-error">The editor could not load.</p>
    {/await}
  </div>
{:else if body?.mode === "formData"}
  <FormDataTable
    fields={body.fields}
    {variables}
    disabled={readonly}
    {onerror}
    onchange={(fields: FormField[]) => asBody({ ...body, fields })}
  />
{:else if body?.mode === "urlencoded"}
  <KeyValueTable
    rows={body.fields}
    label="URL encoded"
    addLabel="Add field"
    checkboxLabel={sendLabel}
    {variables}
    disabled={readonly}
    onchange={(fields: KeyValue[]) => asBody({ ...body, fields })}
  />
{:else if body?.mode === "binary"}
  <div class="card">
    <FileChoice
      value={body.file}
      disabled={readonly}
      {onerror}
      onpick={(file: string) => asBody({ ...body, file })}
    />
  </div>
{/if}

<style>
  .top {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    flex-wrap: wrap;
  }

  .modes {
    display: inline-flex;
    padding: 2px;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .modes label {
    height: 28px;
    display: inline-flex;
    align-items: center;
    padding: 0 var(--s-3);
    border-radius: var(--r-small);
    color: var(--text-2);
    white-space: nowrap;
  }

  .modes label:hover {
    color: var(--text-1);
  }

  .modes label.on {
    background: var(--bg-3);
    color: var(--text-1);
  }

  .modes label:has(input:focus-visible) {
    outline: 2px solid var(--focus);
  }

  .modes input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }

  select {
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .format {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    margin-left: auto;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-2);
  }

  .format:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .format:disabled {
    opacity: 0.5;
  }

  .editor {
    flex: 1;
    min-height: 240px;
    border-radius: var(--r-card);
    box-shadow: inset 0 0 0 1px var(--line);
    overflow: hidden;
  }

  .load-error {
    padding: var(--s-3);
    font-size: 12px;
    line-height: 16px;
    color: var(--negative-text);
  }

  .card {
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
</style>
