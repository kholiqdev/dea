<!-- The Vault's password dialogs: unlock, new password, and the wait. -->
<script lang="ts">
  import { untrack } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import type { VaultPromptEvent } from "$lib/ipc/vault";
  import type { VaultPrompts } from "./vault-prompt.svelte";

  let { prompt, prompts }: { prompt: VaultPromptEvent; prompts: VaultPrompts } =
    $props();

  const MIN_CHARS = 8;
  const formId = $derived(`vault-prompt-${prompt.id}`);
  const TOO_SHORT = `Use at least ${MIN_CHARS} characters`;

  const creating = $derived(prompt.kind === "newPassword");
  const wait = $derived(
    prompt.problem?.kind === "retryAfter" ? prompt.problem : null,
  );

  let password = $state("");
  let repeat = $state("");
  let remaining = $state(untrack(() => wait?.seconds ?? 0));
  let mismatch = $state<string | null>(null);

  // The problem the engine reported with the last answer.
  const reported = $derived(
    prompt.problem?.kind === "wrongPassword"
      ? "The Vault password is wrong. Try again."
      : prompt.problem?.kind === "tooShort"
        ? `Use at least ${prompt.problem.min} characters`
        : null,
  );
  const problem = $derived(mismatch ?? reported);

  $effect(() => {
    if (!wait) return;
    const timer = setInterval(() => {
      remaining -= 1;
      if (remaining <= 0) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  });

  function cancel(): void {
    password = "";
    repeat = "";
    prompts.answer(null);
  }

  function submit(event: SubmitEvent): void {
    event.preventDefault();
    if (wait) {
      if (remaining > 0) return;
      // The engine ignores the answer to a wait; the prompt only closes.
      prompts.answer("");
      return;
    }
    if (!password) return;
    if (creating) {
      if ([...password].length < MIN_CHARS) {
        mismatch = TOO_SHORT;
        return;
      }
      if (password !== repeat) {
        mismatch = "The passwords do not match";
        return;
      }
    }
    const answer = password;
    password = "";
    repeat = "";
    prompts.answer(answer);
  }
</script>

<Dialog
  title={wait
    ? "Too many wrong passwords"
    : creating
      ? "Create a Vault password"
      : "Vault password"}
  oncancel={cancel}
>
  <form id={formId} class="form" onsubmit={submit}>
    {#if wait}
      <p class="text">
        You can try again in {remaining}
        {remaining === 1 ? "second" : "seconds"}. This is the wait after 5 wrong
        tries.
      </p>
    {:else if creating}
      <p class="text">
        This is the first production secret on this computer. Linux has no
        system prompt to protect it, so Dea asks for a password of your own.
      </p>
      <label class="row">
        <span class="lbl">New password</span>
        <input
          class="input"
          type="password"
          autocomplete="new-password"
          data-autofocus
          bind:value={password}
        />
      </label>
      <label class="row">
        <span class="lbl">Repeat it</span>
        <input
          class="input"
          type="password"
          autocomplete="new-password"
          bind:value={repeat}
        />
      </label>
    {:else}
      <p class="text">Dea wants to {prompt.reason}.</p>
      <label class="row">
        <span class="lbl">Vault password</span>
        <input
          class="input"
          type="password"
          autocomplete="current-password"
          data-autofocus
          bind:value={password}
        />
      </label>
    {/if}
    {#if problem}<p class="error" role="alert">{problem}</p>{/if}
  </form>
  {#snippet actions()}
    <Button onclick={cancel}>Cancel <kbd class="kc">Esc</kbd></Button>
    <Button
      variant="primary"
      type="submit"
      form={formId}
      disabled={wait ? remaining > 0 : false}
      >{creating ? "Create password" : "Unlock"}</Button
    >
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }

  .text {
    color: var(--text-2);
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }

  .lbl {
    font-size: 12px;
    color: var(--text-3);
  }

  .input {
    height: 36px;
    width: 100%;
    padding: 0 var(--s-3);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-1);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
  }

  .input:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--focus);
  }

  .error {
    font-size: 12px;
    color: var(--negative);
  }

  .kc {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: var(--r-key);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    font: 500 11px/1 var(--font-mono);
    color: var(--text-2);
  }
</style>
