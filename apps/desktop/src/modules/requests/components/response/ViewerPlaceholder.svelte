<!--
  Where a viewer of its own goes: the preview, the virtual viewer of a
  large body, the list of events. Each replaces this with its component.
-->
<script lang="ts">
  let { slot }: { slot: "preview" | "large-body" | "event-list" } = $props();
</script>

<div class="viewer" data-slot={slot}></div>

<style>
  .viewer {
    flex: 1;
    min-height: 0;
  }
</style>
