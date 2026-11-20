<!-- The method, the URL, and Save. -->
<script lang="ts">
  import Save from "@lucide/svelte/icons/save";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import type { RequestDraft } from "../state/drafts.svelte";
  import type { VariableLookup } from "../state/variables.svelte";
  import MethodMenu from "./MethodMenu.svelte";
  import VariableField from "./VariableField.svelte";

  let {
    draft,
    variables,
    readonly,
    onsave,
  }: {
    draft: RequestDraft;
    variables: VariableLookup;
    readonly: boolean;
    onsave: () => void;
  } = $props();

  const saveTitle = `Save request, ${formatChordText("Mod+S", currentPlatform())}`;
</script>

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
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-3) var(--s-4);
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
