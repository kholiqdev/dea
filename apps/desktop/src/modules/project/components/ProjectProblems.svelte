<!--
  Project files Dea cannot load: merge conflicts, and files that cannot be
  read. Dea reloads them on its own once they are fixed.
-->
<script lang="ts">
  import CircleX from "@lucide/svelte/icons/circle-x";
  import GitCompare from "@lucide/svelte/icons/git-compare";
  import type { FileErrorInfo } from "$lib/ipc/bindings/FileErrorInfo";

  let { errors }: { errors: FileErrorInfo[] } = $props();

  const conflicts = $derived(errors.filter((e) => e.mergeConflict));
  const unreadable = $derived(errors.filter((e) => !e.mergeConflict));

  /** The parser's message without the place, which is shown before it. */
  function reason(e: FileErrorInfo): string {
    return e.message.replace(/ at line \d+ column \d+$/, "");
  }
</script>

{#if unreadable.length > 0}
  <section class="group" aria-labelledby="unreadable-title">
    <h3 id="unreadable-title">A file that cannot be read</h3>
    {#each unreadable as e (e.path)}
      <div class="card">
        <CircleX size={16} strokeWidth={1.5} class="negative" />
        <p>
          <code>{e.path}</code>{#if e.line !== null}, line {e.line}{/if}: {reason(
            e,
          )}. The environment is not loaded until the file is fixed.
        </p>
      </div>
    {/each}
  </section>
{/if}

{#if conflicts.length > 0}
  <section class="group" aria-labelledby="conflicts-title">
    <h3 id="conflicts-title">Merge conflicts</h3>
    {#each conflicts as e (e.path)}
      <div class="card">
        <GitCompare size={16} strokeWidth={1.5} class="warning" />
        <p>
          <code>{e.path}</code> has Git conflict markers. Resolve them in your editor;
          Dea reloads the file when it is clean.
        </p>
      </div>
    {/each}
  </section>
{/if}

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-4) var(--s-4) 0;
  }

  h3 {
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-2);
  }

  .card {
    display: flex;
    align-items: flex-start;
    gap: var(--s-2);
    padding: var(--s-4);
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-2);
  }

  .card :global(svg) {
    flex: none;
    margin-top: 2px;
  }

  .card :global(.negative) {
    color: var(--negative-text);
  }

  .card :global(.warning) {
    color: var(--warning-text);
  }

  p {
    line-height: 20px;
  }

  code {
    font-family: var(--font-mono);
  }
</style>
