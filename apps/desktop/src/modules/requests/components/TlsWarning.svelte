<!-- Says so while the request skips TLS verification (F1-REQ-06 #3). -->
<script lang="ts">
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import FileBanner from "@dea/ui/FileBanner.svelte";
  import type { OptionsInfo } from "$lib/ipc/bindings/OptionsInfo";

  let { effective }: { effective: OptionsInfo | null } = $props();
</script>

{#if effective && !effective.verifyTls}
  <FileBanner tone="warning">
    {#snippet icon()}<TriangleAlert size={16} strokeWidth={1.5} />{/snippet}
    TLS verification is off for this request. Anyone on the network path can read
    or change it.
  </FileBanner>
{/if}
