<script lang="ts">
  let { label = "status" }: { label?: string } = $props();
</script>

<button type="button">{label}</button>
