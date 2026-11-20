<!--
  An HTTP method as coloured text. The seven standard methods take their
  `--method-*` token; any other method is neutral and cut to seven characters,
  with the full name in `title`.
-->
<script lang="ts">
  let {
    method,
    size = "default",
  }: { method: string; size?: "small" | "default" } = $props();

  const STANDARD = new Set([
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
  ]);
  const MAX = 7;

  const standard = $derived(STANDARD.has(method.toUpperCase()));
  const text = $derived(
    method.length > MAX ? `${method.slice(0, MAX)}…` : method,
  );
  const color = $derived(
    standard ? `var(--method-${method.toLowerCase()})` : "var(--text-2)",
  );
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
