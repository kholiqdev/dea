<!--
  An HTTP method as coloured text. The seven standard methods take their
  `--method-*` token; any other method is neutral and cut to seven characters,
  with the full name in `title`.
-->
<script lang="ts">
  import { methodColor, methodLabel } from "$lib/http/method";
  let {
    method,
    size = "default",
  }: { method: string; size?: "small" | "default" } = $props();

  const text = $derived(methodLabel(method));
  const color = $derived(methodColor(method));
</script>

<span class="method" class:small={size === "small"} style:color title={method}
  >{text}</span
>

<style>
  .method {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 500;
    line-height: 1.4;
    white-space: nowrap;
  }

  .small {
    font-size: 11px;
  }
</style>
