<!--
  The dialog of one confirmation to send to production: what goes, where,
  and which variables nothing defines, sent as written.
-->
<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import { methodColor } from "$lib/http/method";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import type { SendConfirmEvent } from "$lib/ipc/send";
  import type { SendConfirmations } from "./send-confirm.svelte";

  let {
    confirmation,
    confirmations,
  }: { confirmation: SendConfirmEvent; confirmations: SendConfirmations } =
    $props();

  const escape = chordParts(parseChord("Escape"), currentPlatform());

  let stopAsking = $state(false);
</script>

<Dialog
  title="Send this request to production?"
  oncancel={() => confirmations.answer("cancel")}
>
  <p class="sub">
    {confirmation.secrets
      ? "It uses production secrets and reaches real data"
      : "It reaches real data"}
  </p>
  <dl class="rows">
    <dt>Method</dt>
    <dd>
      <span class="method" style:color={methodColor(confirmation.method)}
        >{confirmation.method}</span
      >
    </dd>
    {#if confirmation.host}
      <dt>Host</dt>
      <dd class="mono url">{confirmation.host}</dd>
    {/if}
    <dt>URL</dt>
    <dd class="mono url">{confirmation.url}</dd>
    <dt>Environment</dt>
    <dd><span class="tab-env env-prod">{confirmation.environment}</span></dd>
    {#if confirmation.undefined.length > 0}
      <dt>Undefined</dt>
      <dd>
        {#each confirmation.undefined as name, i (i)}{i > 0 ? ", " : ""}<code
            >{name}</code
          >{/each}
        <span class="as-written">sent as written</span>
      </dd>
    {/if}
  </dl>
  {#if confirmation.canSkip}
    <label class="skip">
      <input type="checkbox" bind:checked={stopAsking} />
      <span>
        Don't ask again in this session
        <span class="hint">Offered for GET, HEAD and OPTIONS only</span>
      </span>
    </label>
  {/if}
  {#snippet actions()}
    <Button data-autofocus onclick={() => confirmations.answer("cancel")}>
      Cancel
      {#each escape as key, i (i)}<kbd class="kc">{key}</kbd>{/each}
    </Button>
    <Button
      variant="danger-solid"
      onclick={() =>
        confirmations.answer(
          confirmation.canSkip && stopAsking ? "sendAndStopAsking" : "send",
        )}>Send to production</Button
    >
  {/snippet}
</Dialog>

<style>
  .sub {
    color: var(--text-2);
  }

  .rows {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--s-2) var(--s-4);
    margin: var(--s-3) 0 0;
  }

  dt {
    font-size: 12px;
    color: var(--text-3);
  }

  dd {
    margin: 0;
    min-width: 0;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .method {
    font-family: var(--font-mono);
    font-weight: 500;
    overflow-wrap: anywhere;
  }

  code {
    overflow-wrap: anywhere;
  }

  .as-written {
    color: var(--warning-text);
  }

  .url {
    overflow-wrap: anywhere;
  }

  .tab-env {
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 400 11px/1 var(--font-mono);
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .env-prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .skip {
    display: flex;
    gap: var(--s-2);
    align-items: flex-start;
    margin-top: var(--s-3);
  }

  .hint {
    display: block;
    font-size: 12px;
    color: var(--text-3);
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
</style>
