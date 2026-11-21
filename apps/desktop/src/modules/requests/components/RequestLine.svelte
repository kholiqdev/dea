<!--
  The method, the URL, Send (or Cancel once the request left), and Save. In
  production the bar says so where the request leaves: a chip before the
  method, and a danger-solid Send to production.
-->
<script lang="ts">
  import Save from "@lucide/svelte/icons/save";
  import Button from "@dea/ui/Button.svelte";
  import {
    chordParts,
    currentPlatform,
    formatChordText,
    parseChord,
  } from "$lib/keyboard/chord";
  import type { RequestDraft } from "../state/drafts.svelte";
  import type { VariableLookup } from "../state/variables.svelte";
  import MethodMenu from "./MethodMenu.svelte";
  import VariableField from "./VariableField.svelte";

  let {
    draft,
    variables,
    readonly,
    production,
    sending,
    onsave,
    onsend,
    oncancel,
  }: {
    draft: RequestDraft;
    variables: VariableLookup;
    readonly: boolean;
    /** The window's environment is a production one. */
    production: boolean;
    /** The request left, so the button reads Cancel. */
    sending: boolean;
    onsave: () => void;
    onsend: () => void;
    oncancel: () => void;
  } = $props();

  const platform = currentPlatform();
  const saveTitle = `Save request, ${formatChordText("Mod+S", platform)}`;
  const sendKeys = chordParts(parseChord("Mod+Enter"), platform);

  // One button that turns into Cancel, so the focus stays on it.
  function sendOrCancel(e: MouseEvent): void {
    if (!sending) onsend();
    // The second click of a double click on Send is not a Cancel.
    else if (e.detail < 2) oncancel();
  }
</script>

<!--
  Both labels take the same cell, the one not in use hidden, so the button
  keeps the width of the wider one and the URL field does not move.
-->
{#snippet face(
  label: string,
  keys: string[],
  shown: boolean,
  tone: "primary" | "danger" | null,
)}
  <span
    class={["face", { off: !shown }]}
    aria-hidden={shown ? undefined : "true"}
  >
    {label}
    <!-- The keys are shown, not read: the name stays the label. -->
    <span class="keys" aria-hidden="true">
      {#each keys as key, i (i)}{#if shown}<kbd class={["kc", tone]}>{key}</kbd
          >{:else}<span class="kc">{key}</span>{/if}{/each}
    </span>
  </span>
{/snippet}

<div class="line">
  <div class="request">
    {#if production}<span class="tab-env env-prod">production</span>{/if}
    <MethodMenu
      method={draft.draft.method}
      disabled={readonly}
      onchange={(method: string) => draft.setMethod(method)}
    />
    <div class="url">
      <VariableField
        value={draft.urlBar}
        label="URL"
        placeholder="Enter a URL"
        mono
        {variables}
        disabled={readonly}
        oninput={(text: string) => draft.typeUrl(text)}
      />
    </div>
  </div>
  <div class="send">
    <Button
      data-send=""
      variant={sending ? "secondary" : production ? "danger-solid" : "primary"}
      onclick={sendOrCancel}
    >
      <span class="faces">
        {@render face(
          production ? "Send to production" : "Send",
          sendKeys,
          !sending,
          production ? "danger" : "primary",
        )}
        {@render face("Cancel", ["Esc"], sending, null)}
      </span>
    </Button>
  </div>
  <button
    type="button"
    class="save"
    aria-label="Save request"
    title={saveTitle}
    disabled={readonly}
    onclick={onsave}
  >
    <Save size={16} strokeWidth={1.5} />
  </button>
</div>

<style>
  .line {
    height: 56px;
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-4);
  }

  .request {
    flex: 1 1 480px;
    min-width: 0;
    height: 36px;
    display: flex;
    align-items: stretch;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .request:focus-within {
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  /* Production, said where the request leaves, as the tabs say it. */
  .tab-env {
    flex: none;
    align-self: center;
    height: 18px;
    margin-left: var(--s-2);
    padding: 0 6px;
    display: inline-flex;
    align-items: center;
    border-radius: var(--r-key);
    font: 400 11px/1 var(--font-mono);
  }

  .env-prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .url {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    padding: 0 var(--s-1);
  }

  /* The URL field sits in the request line's box, without one of its own. */
  .url :global(.field),
  .url :global(.field:focus-within) {
    background: none;
    box-shadow: none;
  }

  .url :global(.field) {
    font-size: 14px;
  }

  .send {
    flex: none;
    display: flex;
  }

  /* Send and Cancel have the same padding, so only the label sets the width. */
  .send :global(.btn) {
    padding: 0 var(--s-4);
  }

  .faces {
    display: grid;
  }

  .face {
    grid-area: 1 / 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }

  .face.off {
    visibility: hidden;
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

  .kc.primary {
    background: var(--primary-key-bg);
    box-shadow: none;
    color: var(--primary-key-text);
  }

  /* No fill: the text keeps the contrast of the solid negative fill, an outline
     the keycap's shape. */
  .kc.danger {
    background: none;
    box-shadow: inset 0 0 0 1px
      color-mix(in srgb, var(--negative-solid-text) 45%, transparent);
    color: var(--negative-solid-text);
  }

  .save {
    width: 36px;
    height: 36px;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-2);
  }

  .save:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-1);
  }

  .save:disabled {
    opacity: 0.5;
  }
</style>
