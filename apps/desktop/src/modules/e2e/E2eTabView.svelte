<script lang="ts">
  let { tab }: { tab: { title: string } } = $props();
</script>

<h1 class="title">{tab.title}</h1>

<style>
  .title {
    padding: var(--s-8);
    font-size: 15px;
    font-weight: 500;
  }
</style>
