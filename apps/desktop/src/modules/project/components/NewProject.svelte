<script lang="ts">
  import Button from "@dea/ui/Button.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import { createProject, pickFolder, windowActions } from "../api";
  import { welcome } from "../state/welcome.svelte";

  let {
    name: initialName = "",
    location: initialLocation = "",
  }: { name?: string; location?: string } = $props();

  // The form starts from the draft once; Welcome remounts it for a new one.
  // svelte-ignore state_referenced_locally
  let name = $state(initialName);
  // svelte-ignore state_referenced_locally
  let location = $state(initialLocation);
  let error = $state<string | null>(null);
  let busy = $state(false);
  const ready = $derived(name.trim() !== "" && location.trim() !== "" && !busy);

  async function choose(): Promise<void> {
    const picked = await pickFolder().catch(() => null);
    if (picked) location = picked;
  }

  async function create(): Promise<void> {
    busy = true;
    try {
      await createProject(location.trim(), name.trim());
      windowActions.reload();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="page">
  <div class="head">
    <h1>New project</h1>
    <p>A project is a folder you can keep in Git.</p>
  </div>
  <div class="form">
    <div class="frow">
      <span class="lbl">Project name</span>
      <input
        class="input"
        type="text"
        aria-label="Project name"
        bind:value={name}
      />
    </div>
    <div class="frow">
      <span class="lbl">Location</span>
      <span class="pick">
        <input
          class="input mono"
          type="text"
          aria-label="Location"
          bind:value={location}
        />
        <Button size="small" onclick={choose}>Choose…</Button>
      </span>
      <span class="meta">Dea adds a dea.yaml and a few folders here</span>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="buttons">
      <Button variant="primary" disabled={!ready} onclick={create}
        >Create project</Button
      >
      <Button onclick={() => (welcome.page = "start")}>Back</Button>
    </div>
  </div>
</div>

<style>
  .page {
    max-width: 760px;
    margin: 0 auto;
    padding: 64px var(--s-6);
    display: flex;
    flex-direction: column;
    gap: var(--s-8);
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  h1 {
    font-size: 24px;
    line-height: 32px;
    font-weight: 500;
  }

  .head p {
    color: var(--text-2);
  }

  .form {
    max-width: 640px;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }

  .frow {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    align-items: center;
    row-gap: var(--s-1);
  }

  .lbl {
    color: var(--text-2);
  }

  .pick {
    display: flex;
    gap: var(--s-2);
    align-items: center;
  }

  .input {
    height: 36px;
    width: 100%;
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

  .mono {
    font-family: var(--font-mono);
  }

  .meta {
    grid-column: 2;
    font-size: 12px;
    color: var(--text-3);
  }

  .buttons {
    display: flex;
    gap: var(--s-2);
    padding-left: 180px;
  }

  .error {
    padding-left: 180px;
    color: var(--negative-text);
  }
</style>
