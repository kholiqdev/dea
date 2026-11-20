<script lang="ts">
  import { onMount } from "svelte";
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView, keymap, lineNumbers } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { lintGutter, linter, type Diagnostic } from "@codemirror/lint";
  import { json } from "@codemirror/lang-json";
  import { xml } from "@codemirror/lang-xml";
  import { html } from "@codemirror/lang-html";
  import { javascript } from "@codemirror/lang-javascript";
  import { tags } from "@lezer/highlight";
  import { formatJson, lintJson } from "./json";
  import {
    variableExtension,
    variableCompartment,
    variableStateExtension,
  } from "./variable-extension";
  import type { VariableState } from "./variables";

  interface Props {
    value: string;
    language: "json" | "xml" | "html" | "javascript" | "text";
    label: string;
    readonly?: boolean;
    variables?: VariableState;
    onchange?: (value: string) => void;
  }

  let {
    value,
    language,
    label,
    readonly = false,
    variables,
    onchange,
  }: Props = $props();

  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let text = $state("");

  const languageCompartment = new Compartment();
  const lintCompartment = new Compartment();
  const readonlyCompartment = new Compartment();

  function languageExtension(name: Props["language"]) {
    switch (name) {
      case "json":
        return json();
      case "xml":
        return xml();
      case "html":
        return html();
      case "javascript":
        return javascript();
      default:
        return [];
    }
  }

  const highlightStyle = HighlightStyle.define([
    { tag: tags.propertyName, color: "var(--syn-key)" },
    { tag: [tags.string, tags.special(tags.string)], color: "var(--syn-str)" },
    { tag: tags.number, color: "var(--syn-num)" },
    { tag: [tags.bool, tags.null, tags.atom], color: "var(--syn-bool)" },
    {
      tag: [tags.keyword, tags.tagName, tags.typeName],
      color: "var(--syn-kw)",
    },
    {
      tag: [tags.punctuation, tags.bracket, tags.separator, tags.angleBracket],
      color: "var(--syn-punct)",
    },
  ]);

  const theme = EditorView.theme({
    "&": {
      backgroundColor: "var(--bg-1)",
      color: "var(--text-1)",
      fontFamily: "var(--font-mono)",
      height: "100%",
    },
    ".cm-scroller": { fontFamily: "var(--font-mono)" },
    ".cm-gutters": {
      backgroundColor: "var(--bg-2)",
      color: "var(--text-3)",
      borderRight: "1px solid var(--line)",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-cursor": { borderLeftColor: "var(--text-1)" },
    ".cm-var": { color: "var(--syn-kw)" },
    ".cm-var-undefined": { color: "var(--negative-text)" },
    ".cm-tooltip": {
      backgroundColor: "var(--bg-2)",
      color: "var(--text-1)",
      border: "1px solid var(--line)",
    },
    ".cm-var-tooltip": { padding: "4px 8px" },
    ".cm-var-tooltip-detail": { color: "var(--text-2)" },
  });

  const jsonLinter = [
    lintGutter(),
    linter(
      (editor): Diagnostic[] => {
        const error = lintJson(editor.state.doc.toString());
        if (!error) return [];
        const line = editor.state.doc.line(
          Math.min(error.line, editor.state.doc.lines),
        );
        const from = Math.min(line.from + error.column - 1, line.to);
        return [
          {
            from,
            to: Math.min(from + 1, editor.state.doc.length),
            severity: "error",
            message: error.message,
          },
        ];
      },
      { delay: 200 },
    ),
  ];

  function lintExtension(name: Props["language"]) {
    return name === "json" ? jsonLinter : [];
  }

  function format(editor: EditorView): boolean {
    if (language !== "json") return false;
    const formatted = formatJson(editor.state.doc.toString());
    if (formatted === null) return true;
    editor.dispatch({
      changes: { from: 0, to: editor.state.doc.length, insert: formatted },
    });
    return true;
  }

  const status = $derived(language === "json" ? lintJson(text) : null);

  onMount(() => {
    text = value;
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          history(),
          keymap.of([
            { key: "Mod-Shift-f", run: format },
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          syntaxHighlighting(highlightStyle),
          theme,
          lintCompartment.of(lintExtension(language)),
          variableExtension(variables),
          languageCompartment.of(languageExtension(language)),
          readonlyCompartment.of([
            EditorState.readOnly.of(readonly),
            EditorView.editable.of(!readonly),
          ]),
          EditorView.contentAttributes.of({ "aria-label": label }),
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) return;
            text = update.state.doc.toString();
            onchange?.(text);
          }),
        ],
      }),
    });
    return () => view?.destroy();
  });

  $effect(() => {
    const next = value;
    if (view && next !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: next },
      });
    }
  });

  $effect(() => {
    view?.dispatch({
      effects: variableCompartment.reconfigure(
        variableStateExtension(variables),
      ),
    });
  });

  $effect(() => {
    view?.dispatch({
      effects: [
        languageCompartment.reconfigure(languageExtension(language)),
        lintCompartment.reconfigure(lintExtension(language)),
      ],
    });
  });

  $effect(() => {
    view?.dispatch({
      effects: readonlyCompartment.reconfigure([
        EditorState.readOnly.of(readonly),
        EditorView.editable.of(!readonly),
      ]),
    });
  });

  /** For tests only. */
  export function getView(): EditorView | undefined {
    return view;
  }
</script>

<div class="code-editor">
  <div class="surface" bind:this={host}></div>
  <p class="status" aria-live="polite">
    {status ? `Line ${status.line}: ${status.message}` : ""}
  </p>
</div>

<style>
  .code-editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .surface {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .status {
    margin: 0;
    padding: 4px 8px;
    min-height: 1.5em;
    border-top: 1px solid var(--line);
    background-color: var(--bg-2);
    color: var(--negative-text);
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
