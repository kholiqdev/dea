<!-- Edits one cookie. The backend keeps the original when it refuses an edit. -->
<script lang="ts">
  import { untrack } from "svelte";
  import Button from "@dea/ui/Button.svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import type { CookieInfo } from "$lib/ipc/bindings/CookieInfo";
  import { errorMessage } from "$lib/ipc/errors";

  let {
    cookie,
    onsave,
    oncancel,
  }: {
    cookie: CookieInfo;
    onsave: (cookie: CookieInfo) => Promise<void>;
    oncancel: () => void;
  } = $props();

  const pad = (n: number) => String(n).padStart(2, "0");

  /** An RFC 3339 time as the local time a `datetime-local` input takes. */
  function toLocal(expires: string | null): string {
    if (expires === null) return "";
    const d = new Date(expires);
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  // The dialog is made anew for each cookie, so the first one is all it needs.
  const first = untrack(() => $state.snapshot(cookie));
  let value = $state(first.value);
  let path = $state(first.path);
  let session = $state(first.expires === null);
  let expires = $state(toLocal(first.expires));
  let secure = $state(first.secure);
  let httpOnly = $state(first.httpOnly);
  let error = $state<string | null>(null);
  let saving = $state(false);

  function expiry(): string | null {
    if (session || expires === "") return null;
    // The time was not touched: keep the exact one, seconds included.
    if (expires === toLocal(first.expires)) return first.expires;
    return new Date(expires).toISOString();
  }

  async function save(): Promise<void> {
    saving = true;
    error = null;
    try {
      await onsave({
        ...first,
        value,
        path,
        expires: expiry(),
        secure,
        httpOnly,
      });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog title="Edit cookie {first.name}" {oncancel}>
  <div class="form">
    <label class="field">
      <span>Value</span>
      <input class="input" type="text" bind:value />
    </label>
    <label class="field">
      <span>Path</span>
      <input class="input" type="text" bind:value={path} />
    </label>
    <label class="field">
      <span>Expires</span>
      <input
        class="input"
        type="datetime-local"
        bind:value={expires}
        disabled={session}
      />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={session} />
      <span>Session cookie</span>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={secure} />
      <span>Secure</span>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={httpOnly} />
      <span>HttpOnly</span>
    </label>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </div>
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <Button variant="primary" disabled={saving} onclick={() => void save()}>
      Save
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
    font-size: 12px;
    color: var(--text-3);
  }

  .input {
    height: 32px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-ctl);
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: 13px;
  }

  .input:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .input:disabled {
    opacity: 0.5;
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    font-size: 13px;
    color: var(--text-2);
  }

  .error {
    margin: 0;
    color: var(--negative-text);
    font-size: 13px;
  }
</style>
