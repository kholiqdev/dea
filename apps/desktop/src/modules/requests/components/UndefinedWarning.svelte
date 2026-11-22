<!--
  Why a send stopped: the request uses variables that nothing defines, and
  Dea would send them as written (F1-REQ-05 #3). It can point at where the
  first one is used, and a second send goes past it.
-->
<script lang="ts">
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import { chordParts, currentPlatform, parseChord } from "$lib/keyboard/chord";
  import type { Usage } from "../lib/usage";

  let {
    names,
    environment,
    usage,
    ondefine,
    onshow,
    onsendanyway,
  }: {
    names: string[];
    /** The environment in use; null for none. */
    environment: string | null;
    /** Where the first name is used; null when that is not known. */
    usage: Usage | null;
    ondefine: () => void;
    /** Takes the user to `usage`. */
    onshow: () => void;
    onsendanyway: () => void;
  } = $props();

  const PLACES = {
    url: "URL",
    params: "Params",
    headers: "Headers",
    body: "Body",
  } as const;
  const sendKeys = chordParts(parseChord("Mod+Enter"), currentPlatform());

  const one = $derived(names.length === 1);
  const where = $derived(
    environment === null
      ? "in this collection"
      : `in ${environment} or in this collection`,
  );
</script>

<FileBanner
  tone="warning"
  actions={[
    { label: "Define it", onclick: ondefine },
    ...(usage
      ? [
          {
            label: `Show in ${PLACES[usage.section]}`,
            variant: "ghost" as const,
            onclick: onshow,
          },
        ]
      : []),
    {
      label: "Send anyway",
      variant: "ghost",
      keys: sendKeys,
      onclick: onsendanyway,
    },
  ]}
>
  {#snippet icon()}<TriangleAlert size={16} strokeWidth={1.5} />{/snippet}
  {#each names as name, i (i)}{#if i > 0}{i === names.length - 1
        ? " and "
        : ", "}{/if}<code>{name}</code>{/each}
  {one ? "is" : "are"} not defined {where}.
  {one ? "It is" : "They are"} sent as written.
</FileBanner>
