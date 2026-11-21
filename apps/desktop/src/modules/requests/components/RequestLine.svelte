<!-- The method, the URL, Send (or Cancel while sending), and Save. -->
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
    sending,
    onsave,
    onsend,
    oncancel,
  }: {
    draft: RequestDraft;
    variables: VariableLookup;
    readonly: boolean;
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
{#snippet face(label: string, keys: string[], shown: boolean)}
  <span
    class={["face", { off: !shown }]}
    aria-hidden={shown ? undefined : "true"}
  >
    {label}
    <!-- The keys are shown, not read: the name stays Send or Cancel. -->
    <span class="keys" aria-hidden="true">
      {#each keys as key, i (i)}{#if shown}<kbd
            class={["kc", { primary: !sending }]}>{key}</kbd
          >{:else}<span class="kc">{key}</span>{/if}{/each}
    </span>
  </span>
{/snippet}

<div class="line">
  <div class="request">
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
      variant={sending ? "secondary" : "primary"}
      onclick={sendOrCancel}
    >
      <span class="faces">
        {@render face("Send", sendKeys, !sending)}
        {@render face("Cancel", ["Esc"], sending)}
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
