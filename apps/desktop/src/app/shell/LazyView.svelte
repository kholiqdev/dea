<!-- Renders a contribution's view once its code has loaded. -->
<script lang="ts">
  import type { AnyComponent, Loader } from "../contributions/types";

  let {
    loader,
    props = {},
  }: { loader: Loader<AnyComponent>; props?: Record<string, unknown> } =
    $props();

  const loaded = $derived(loader());
</script>

{#await loaded then mod}
  {@const View = mod.default}
  <View {...props} />
{/await}
