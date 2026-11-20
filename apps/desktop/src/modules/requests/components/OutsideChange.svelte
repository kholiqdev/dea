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
  import type { RequestDraft } from "../state/drafts.svelte";

  let { draft, onclose }: { draft: RequestDraft; onclose: () => void } =
    $props();

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
    This request changed on disk.
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
    This request was deleted on disk.
  </FileBanner>
{/if}
