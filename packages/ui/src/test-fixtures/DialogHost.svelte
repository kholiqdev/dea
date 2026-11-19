<script lang="ts">
  import Dialog from "../Dialog.svelte";

  let open = $state(false);
  let cancelled = $state(0);
</script>

<button type="button" onclick={() => (open = true)}>Open</button>
<button type="button">Outside</button>
<output>{cancelled}</output>
{#if open}
  <Dialog
    title="Save changes?"
    oncancel={() => {
      cancelled += 1;
      open = false;
    }}
  >
    <p>Body</p>
    {#snippet actions()}
      <button type="button" onclick={() => (open = false)}>Cancel</button>
      <button type="button" data-autofocus>Save</button>
    {/snippet}
  </Dialog>
{/if}
