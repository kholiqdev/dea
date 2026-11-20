<!--
  A file field: the chosen file's name, a note when it is outside the
  project or not on this device, and the button that opens the file chooser.
  The request file holds a path inside the project or a `local:` reference.
-->
<script lang="ts">
  import type { FileRef } from "$lib/ipc/bindings/FileRef";
  import { errorMessage } from "$lib/ipc/errors";
  import { describeFileRef, pickRequestFile } from "../api";

  let {
    value,
    disabled = false,
    label,
    onpick,
    onerror,
  }: {
    /** The path or `local:` reference; empty while nothing is chosen. */
    value: string;
    disabled?: boolean;
    /** The button's accessible name, which starts with its text. */
    label?: string;
    onpick: (value: string) => void;
    onerror?: (message: string) => void;
  } = $props();

  let info = $state<FileRef | null>(null);

  // What the reference points at; a file just chosen is already known.
  $effect(() => {
    const current = value;
    if (current === "") {
      info = null;
      return;
    }
    if (info?.value === current) return;
    let stale = false;
    describeFileRef(current).then(
      (ref) => {
        if (!stale) info = ref;
      },
      (e: unknown) => onerror?.(errorMessage(e)),
    );
    return () => {
      stale = true;
    };
  });

  async function choose(): Promise<void> {
    try {
      const picked = await pickRequestFile();
      if (!picked) return;
      info = picked;
      onpick(picked.value);
    } catch (e) {
      onerror?.(errorMessage(e));
    }
  }
</script>

<div class="choice">
  <div class="line">
    {#if info?.missing}
      <span class="missing">Missing</span>
    {:else if info}
      <span class="name" title={info.name}>{info.name}</span>
    {/if}
    <button
      type="button"
      class="choose"
      aria-label={label}
      {disabled}
      onclick={() => void choose()}>Choose…</button
    >
  </div>
  {#if info?.missing && info.value.startsWith("local:")}
    <p class="note">
      This file was picked on another device. Choose it again on this one.
    </p>
  {:else if info?.outside && !info.missing}
    <p class="note warning">
      {info.name} is outside the project, so it is not committed. Teammates pick their
      own copy.
    </p>
  {/if}
</div>

<style>
  .choice {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
    padding: var(--s-1) var(--s-2);
    min-width: 0;
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    min-width: 0;
  }

  .name,
  .missing {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    color: var(--text-1);
  }

  .missing {
    font-family: var(--font-ui);
    color: var(--negative-text);
  }

  .choose {
    height: 28px;
    flex: none;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-1);
  }

  .choose:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  .choose:disabled {
    opacity: 0.5;
  }

  .note {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-3);
  }

  .warning {
    color: var(--warning-text);
  }
</style>
