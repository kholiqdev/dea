<script lang="ts">
  import Trash from "@lucide/svelte/icons/trash-2";
  import { vaultApi } from "$app/contributions/vault-api";
  import type { EnvironmentDraft } from "../state/environments.svelte";

  let {
    draft,
    readonly = false,
    ondelete,
  }: {
    draft: EnvironmentDraft;
    readonly?: boolean;
    ondelete: () => void;
  } = $props();

  const COLOURS = [
    ["slate", "Slate"],
    ["blue", "Blue"],
    ["green", "Green"],
    ["amber", "Amber"],
    ["silver", "Silver"],
  ] as const;

  const env = $derived(draft.draft);

  // Production on disk can only be turned off after signing in (F1-FON-07 #4).
  async function toggleProduction(): Promise<void> {
    if (env.production && draft.saved?.production) {
      const ok = await vaultApi().authorize(
        `turn off production for ${draft.saved.name}`,
      );
      if (!ok) return;
    }
    env.production = !env.production;
  }
</script>

<aside class="inspector" aria-label="Environment">
  <div class="title">
    <h2>{env.name}</h2>
    <span class="meta">
      Applies to request tabs only. Database connections and hosts keep their
      own environment.
    </span>
  </div>

  <div class="group">
    <h3 class="field-label">Name</h3>
    <input
      class="input"
      aria-label="Name"
      value={env.name}
      oninput={(e) => draft.setName(e.currentTarget.value)}
      {readonly}
    />
  </div>

  <div class="group">
    <h3 class="field-label" id="colour-label">Colour</h3>
    <div class="swatches" role="radiogroup" aria-labelledby="colour-label">
      {#each COLOURS as [value, label] (value)}
        <button
          type="button"
          role="radio"
          class="swatch"
          aria-checked={(env.color ?? "slate") === value}
          aria-label={label}
          title={label}
          style:background="var(--env-{value})"
          disabled={readonly}
          onclick={() => (env.color = value)}
        ></button>
      {/each}
    </div>
    <span class="meta">Red is kept for production</span>
  </div>

  <div class="group">
    <h3 class="field-label">Production</h3>
    <div class="row">
      <span class="t2">Treat as production</span>
      <button
        type="button"
        role="switch"
        class="switch"
        aria-checked={env.production}
        aria-label="Treat as production"
        disabled={readonly}
        onclick={toggleProduction}><span class="knob"></span></button
      >
    </div>
    <span class="meta">
      Production asks before sending and shows a red label. Turning this off
      again asks you to sign in.
    </span>
  </div>

  {#if !readonly}
    <button type="button" class="delete" onclick={ondelete}>
      <Trash size={16} strokeWidth={1.5} /><span>Delete {env.name}…</span>
    </button>
  {/if}
</aside>

<style>
  .inspector {
    width: 320px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--s-6);
    padding: var(--s-4);
    overflow: auto;
    background: var(--bg-0);
    border-left: 1px solid var(--line);
  }

  .title {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }

  h2 {
    font-size: 15px;
    line-height: 20px;
    font-weight: 500;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .field-label {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-2);
  }

  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .t2 {
    color: var(--text-2);
  }

  .input {
    height: 36px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .input:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .swatches {
    display: flex;
    gap: var(--s-3);
  }

  .swatch {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 14px;
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .swatch[aria-checked="true"] {
    box-shadow:
      0 0 0 2px var(--bg-0),
      0 0 0 4px var(--text-1);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .switch {
    width: 36px;
    height: 20px;
    padding: 2px;
    border: 0;
    border-radius: 10px;
    background: var(--n-2);
  }

  .switch[aria-checked="true"] {
    background: var(--negative);
  }

  .switch:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .knob {
    display: block;
    width: 16px;
    height: 16px;
    border-radius: 8px;
    background: var(--text-1);
    transition: transform var(--t-fast) var(--ease);
  }

  .switch[aria-checked="true"] .knob {
    transform: translateX(16px);
  }

  .delete {
    height: 36px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--negative-text);
  }
</style>
