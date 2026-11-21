<!-- Settings > Requests: how the request editor saves. -->
<script lang="ts">
  import type { ResponseLayout } from "$lib/ipc/bindings/ResponseLayout";
  import { errorMessage } from "$lib/ipc/errors";
  import { settings } from "$lib/settings/settings.svelte";
  import { setResponseLayout } from "../state/layout";

  const POSITIONS: { value: ResponseLayout; label: string }[] = [
    { value: "right", label: "Right" },
    { value: "below", label: "Below" },
  ];

  let error = $state<string | null>(null);

  async function change(save: () => Promise<void>): Promise<void> {
    try {
      await save();
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<div class="page">
  <header class="page-head">
    <h1>Requests</h1>
  </header>

  <section class="group" aria-labelledby="saving-head">
    <h2 id="saving-head" class="group-title">Saving</h2>
    <div class="card">
      <div class="frow">
        <span class="lbl">Autosave</span>
        <div class="field">
          <label class="switch">
            <input
              type="checkbox"
              role="switch"
              aria-describedby="autosave-hint"
              checked={settings.requests.autosave}
              onchange={(e) =>
                void change(() =>
                  settings.update({
                    requests: { autosave: e.currentTarget.checked },
                  }),
                )}
            />
            Save requests automatically
          </label>
          <span id="autosave-hint" class="meta"
            >Edits are saved a second after you stop typing.</span
          >
        </div>
      </div>
    </div>
  </section>

  <section class="group" aria-labelledby="layout-head">
    <h2 id="layout-head" class="group-title">Layout</h2>
    <div class="card">
      <div class="frow">
        <span class="lbl" id="position-label">Response position</span>
        <div class="field">
          <div
            class="segmented"
            role="radiogroup"
            aria-labelledby="position-label"
          >
            {#each POSITIONS as position (position.value)}
              <label class="segment">
                <input
                  type="radio"
                  name="response-position"
                  value={position.value}
                  checked={settings.requests.responseLayout === position.value}
                  onchange={() =>
                    void change(() => setResponseLayout(position.value))}
                />
                {position.label}
              </label>
            {/each}
          </div>
          <span class="meta"
            >A narrow window always puts the response below.</span
          >
        </div>
      </div>
    </div>
  </section>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .page {
    max-width: 760px;
    padding: var(--s-8);
    display: flex;
    flex-direction: column;
    gap: var(--s-8);
  }

  .page-head h1 {
    font-size: 24px;
    line-height: 32px;
    font-weight: 500;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .group-title {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    padding: var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .frow {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    align-items: start;
    column-gap: var(--s-4);
  }

  .lbl {
    line-height: 20px;
    color: var(--text-2);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    align-items: flex-start;
  }

  .switch {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    color: var(--text-1);
  }

  .segmented {
    display: inline-flex;
    padding: 2px;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
  }

  .segment {
    position: relative;
    padding: 4px 12px;
    border-radius: var(--r-small);
    color: var(--text-2);
  }

  .segment input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
  }

  .segment:has(input:checked) {
    background: var(--bg-1);
    color: var(--text-1);
  }

  .segment:has(input:focus-visible) {
    outline: 2px solid var(--focus);
  }

  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .error {
    font-size: 12px;
    line-height: 16px;
    color: var(--negative-text);
  }
</style>
