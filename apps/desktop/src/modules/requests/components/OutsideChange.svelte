<!--
  What happened to the request's file outside Dea while it had edits, and
  what can be done: keep the edits or take the file, or save it again.
-->
<script lang="ts">
  import FileExclamationPoint from "@lucide/svelte/icons/file-exclamation-point";
  import FileX from "@lucide/svelte/icons/file-x";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import { shellApi } from "$app/contributions/shell-api";
  import { errorMessage } from "$lib/ipc/errors";

  /** What the banner needs of a draft: a request's, or a folder's. */
  interface Outside {
    outside: "changed" | "deleted" | null;
    keepMine(): void;
    takeTheirs(): Promise<void>;
    save(): Promise<void>;
  }

  let {
    draft,
    onclose,
    noun = "request",
  }: { draft: Outside; onclose: () => void; noun?: string } = $props();

  function attempt(action: Promise<void>): void {
    action.catch((e: unknown) => shellApi().toast(errorMessage(e)));
  }
</script>

{#if draft.outside === "changed"}
  <FileBanner
    tone="warning"
    actions={[
      { label: "Keep mine", onclick: () => draft.keepMine() },
      { label: "Take theirs", onclick: () => attempt(draft.takeTheirs()) },
    ]}
  >
    {#snippet icon()}<FileExclamationPoint
        size={16}
        strokeWidth={1.5}
      />{/snippet}
    This {noun} changed on disk.
  </FileBanner>
{:else if draft.outside === "deleted"}
  <FileBanner
    tone="warning"
    actions={[
      { label: "Save again", onclick: () => attempt(draft.save()) },
      { label: "Close tab", variant: "ghost", onclick: onclose },
    ]}
  >
    {#snippet icon()}<FileX size={16} strokeWidth={1.5} />{/snippet}
    This {noun} was deleted on disk.
  </FileBanner>
{/if}
