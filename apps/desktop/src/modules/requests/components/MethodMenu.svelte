<!--
  The request's method: one of the standard seven from the menu, or any
  other HTTP token typed after choosing Custom… (F1-REQ-02 #1). Enter or
  leaving the field takes a valid token; Escape takes nothing.
-->
<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import { tick } from "svelte";
  import Menu, { type MenuItem } from "@dea/ui/Menu.svelte";
  import MethodBadge from "./MethodBadge.svelte";

  let {
    method,
    disabled = false,
    onchange,
  }: {
    method: string;
    disabled?: boolean;
    onchange: (method: string) => void;
  } = $props();

  const STANDARD = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
  // The characters of an HTTP token (RFC 9110, section 5.6.2).
  const TOKEN = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;

  let open = $state(false);
  // The text of a custom method being typed; null while the menu shows.
  let custom = $state<string | null>(null);
  let root = $state<HTMLElement>();

  const valid = $derived(custom !== null && TOKEN.test(custom));
  const items: MenuItem[] = $derived([
    ...STANDARD.map((m) => ({
      id: m,
      label: m,
      checked: m === method,
      onselect: () => onchange(m),
    })),
    {
      id: "custom",
      label: "Custom…",
      checked: !STANDARD.includes(method),
      onselect: () => void startCustom(),
    },
  ]);

  async function startCustom(): Promise<void> {
    custom = STANDARD.includes(method) ? "" : method;
    await tick();
    root?.querySelector("input")?.focus();
  }

  async function finish(apply: boolean): Promise<void> {
    if (apply && custom !== null && valid) onchange(custom);
    custom = null;
    await tick();
    root?.querySelector<HTMLElement>("button")?.focus();
  }

  /** Leaving the field keeps a valid method, as Enter does. */
  function leave(): void {
    if (custom !== null && valid) onchange(custom);
    custom = null;
  }

  function onkeydown(e: KeyboardEvent): void {
    if (e.key === "Enter" && valid) {
      e.preventDefault();
      void finish(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      void finish(false);
    }
  }
</script>

{#snippet label()}
  <span class="sr">Method: {method}</span>
  <span class="shown" aria-hidden="true"><MethodBadge {method} /></span>
  <ChevronDown size={16} strokeWidth={1.5} color="var(--text-3)" />
{/snippet}

<div class="method" bind:this={root}>
  {#if custom !== null}
    <input
      class="custom"
      aria-label="Custom method"
      aria-invalid={custom !== "" && !valid}
      autocomplete="off"
      spellcheck="false"
      bind:value={custom}
      {onkeydown}
      onblur={leave}
    />
  {:else if disabled}
    <button type="button" class="locked" disabled>{@render label()}</button>
  {:else}
    <Menu bind:open {items} menuLabel="Method" trigger={label} />
  {/if}
</div>

<style>
  .method {
    display: flex;
    align-items: center;
    padding: 0 var(--s-1);
    border-right: 1px solid var(--line);
  }

  .shown {
    display: inline-flex;
  }

  .locked {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 var(--s-2);
    border: 0;
    background: none;
    opacity: 0.5;
  }

  .custom {
    width: 112px;
    height: 28px;
    padding: 0 var(--s-2);
    border: 0;
    border-radius: var(--r-small);
    background: var(--bg-1);
    box-shadow: inset 0 0 0 1px var(--focus);
    outline: none;
    font: 500 13px var(--font-mono);
    color: var(--text-1);
  }

  .custom[aria-invalid="true"] {
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
