<!-- What a window without a project shows (F1-FON-03). -->
<script lang="ts">
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Import from "@lucide/svelte/icons/import";
  import Plus from "@lucide/svelte/icons/plus";
  import { errorMessage, isIpcError } from "$lib/ipc/errors";
  import { openProject, pickFolder, windowActions } from "./api";
  import NewProject from "./components/NewProject.svelte";
  import NotAProject from "./components/NotAProject.svelte";
  import RecentProjects from "./components/RecentProjects.svelte";
  import { folderName, newProjectPage, welcome } from "./state/welcome.svelte";

  let error = $state<string | null>(null);
  const notAProject = $derived(
    typeof welcome.page === "object" && "notAProject" in welcome.page
      ? welcome.page.notAProject
      : null,
  );
  const draft = $derived(
    typeof welcome.page === "object" && "new" in welcome.page
      ? welcome.page.new
      : null,
  );

  async function openFolder(): Promise<void> {
    error = null;
    const root = await pickFolder().catch(() => null);
    if (!root) return;
    try {
      await openProject(root);
      windowActions.reload();
    } catch (e) {
      if (isIpcError(e) && e.kind === "notAProject")
        welcome.page = { notAProject: root };
      else error = errorMessage(e);
    }
  }
</script>

{#if draft}
  <!-- A new draft (from "Create a project there") starts a fresh form. -->
  {#key draft}
    <NewProject name={draft.name} location={draft.location} />
  {/key}
{:else}
  <div class="page">
    <span class="logo" role="img" aria-label="Dea"></span>
    <div class="head">
      <h1>Start a project</h1>
      <p>
        A project is a folder with your requests, connections and hosts.
        Everything is saved on this device and nothing is uploaded.
      </p>
    </div>
    <div class="cards">
      <button
        type="button"
        class="card"
        onclick={() => (welcome.page = newProjectPage())}
      >
        <Plus size={20} strokeWidth={1.5} />
        <span class="text">
          <span class="title">Start empty</span>
          <span class="meta">Name the project and pick its folder</span>
        </span>
      </button>
      <button type="button" class="card" disabled>
        <Import size={20} strokeWidth={1.5} />
        <span class="text">
          <span class="title">Import what you have</span>
          <span class="meta">Postman, OpenAPI, cURL, DBeaver or SSH config</span
          >
          <span class="meta soon">Comes with import in a later version</span>
        </span>
      </button>
      <button type="button" class="card" onclick={openFolder}>
        <FolderOpen size={20} strokeWidth={1.5} />
        <span class="text">
          <span class="title">Open a project folder</span>
          <span class="meta">A folder that already has a dea.yaml</span>
        </span>
      </button>
    </div>
    {#if notAProject}
      <NotAProject
        path={notAProject}
        oncreate={() =>
          (welcome.page = {
            new: { name: folderName(notAProject), location: notAProject },
          })}
        onchoose={openFolder}
      />
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <RecentProjects onerror={(m: string) => (error = m)} />
  </div>
{/if}

<style>
  .page {
    max-width: 760px;
    margin: 0 auto;
    padding: 64px var(--s-6);
    display: flex;
    flex-direction: column;
    gap: var(--s-6);
  }

  .logo {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
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

  .cards {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--s-3);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    padding: var(--s-4);
    border: 0;
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
    text-align: left;
  }

  .card:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  .card:disabled {
    cursor: default;
    color: var(--text-3);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }

  .title {
    font-weight: 500;
  }

  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .soon {
    color: var(--text-2);
  }

  .error {
    color: var(--negative-text);
  }
</style>
