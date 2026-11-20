<!-- Settings > Requests: how the request editor saves. -->
<script lang="ts">
  import { errorMessage } from "$lib/ipc/errors";
  import { settings } from "$lib/settings/settings.svelte";

  let error = $state<string | null>(null);

  async function toggle(autosave: boolean): Promise<void> {
    try {
      await settings.update({ requests: { autosave } });
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
              onchange={(e) => void toggle(e.currentTarget.checked)}
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
