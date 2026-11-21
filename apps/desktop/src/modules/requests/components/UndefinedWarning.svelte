<!--
  Why a send stopped: the request uses variables that nothing defines, and
  Dea would send them as written (F1-REQ-05 #3).
-->
<script lang="ts">
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import FileBanner from "@dea/ui/FileBanner.svelte";

  let {
    names,
    environment,
    ondefine,
    onsendanyway,
  }: {
    names: string[];
    /** The environment in use; null for none. */
    environment: string | null;
    ondefine: () => void;
    onsendanyway: () => void;
  } = $props();

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
    { label: "Send anyway", variant: "ghost", onclick: onsendanyway },
  ]}
>
  {#snippet icon()}<TriangleAlert size={16} strokeWidth={1.5} />{/snippet}
  {#each names as name, i (i)}{#if i > 0}{i === names.length - 1
        ? " and "
        : ", "}{/if}<code>{name}</code>{/each}
  {one ? "is" : "are"} not defined {where}.
  {one ? "It is" : "They are"} sent as written.
</FileBanner>
