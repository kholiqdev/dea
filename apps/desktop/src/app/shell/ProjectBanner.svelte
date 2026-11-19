<!--
  What is wrong with the project as a whole, above the work area: its
  folder is gone, or its dea.yaml cannot be read.
-->
<script lang="ts">
  import CircleX from "@lucide/svelte/icons/circle-x";
  import FolderX from "@lucide/svelte/icons/folder-x";
  import GitCompare from "@lucide/svelte/icons/git-compare";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import { closeProject, openProject, pickFolder } from "$lib/ipc/project";
  import { toasts } from "../prompts/toasts.svelte";
  import { blockedByUnsaved } from "../prompts/unsaved";
  import { shellContext } from "./context";
  import { windowReload } from "./window-reload";

  const ctx = shellContext();
  const info = $derived(ctx.project.info);
  const manifest = $derived(info?.errors.find((e) => e.path === "dea.yaml"));

  function attempt(action: () => Promise<unknown>): void {
    action().catch((e: unknown) => toasts.show(errorMessage(e)));
  }

  async function locate(): Promise<void> {
    const root = await pickFolder();
    if (!root) return;
    await openProject(root);
    windowReload.run();
  }

  async function close(): Promise<void> {
    if (blockedByUnsaved(ctx.tabs)) return;
    await closeProject();
    windowReload.run();
  }
</script>

{#if info?.missing}
  <FileBanner
    tone="negative"
    actions={[
      { label: "Locate", onclick: () => attempt(locate) },
      {
        label: "Close project",
        variant: "ghost",
        onclick: () => attempt(close),
      },
    ]}
  >
    {#snippet icon()}<FolderX size={16} strokeWidth={1.5} />{/snippet}
    Folder not found: <code>{info.root}</code>. It was moved, renamed or deleted
    outside Dea.
  </FileBanner>
{:else if manifest?.mergeConflict}
  <FileBanner tone="warning">
    {#snippet icon()}<GitCompare size={16} strokeWidth={1.5} />{/snippet}
    <code>dea.yaml</code> has Git conflict markers. Resolve them in your editor; Dea
    reloads the file when it is clean.
  </FileBanner>
{:else if manifest}
  <FileBanner tone="negative">
    {#snippet icon()}<CircleX size={16} strokeWidth={1.5} />{/snippet}
    <code>dea.yaml</code>{#if manifest.line !== null}, line {manifest.line}{/if}:
    {manifest.message}. Dea shows the project as it was until the file is fixed.
  </FileBanner>
{/if}
