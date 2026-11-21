<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { currentPlatform, formatChordText } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import { errorMessage } from "$lib/ipc/errors";
  import { railIcon } from "../modules";
  import { toasts } from "../prompts/toasts.svelte";
  import { shellContext } from "../shell/context";
  import { effectiveShortcuts } from "../contributions/shortcuts";
  import type { TabRecord } from "./tabs.svelte";

  const ctx = shellContext();
  const tabs = $derived(ctx.tabs.tabs);
  const activeId = $derived(ctx.tabs.active?.id);
  const closeChord = $derived.by(() => {
    const b = effectiveShortcuts(ctx.registry, settings.shortcuts).find(
      (s) => s.command === "shell.closeTab",
    );
    return b ? formatChordText(b.chord, currentPlatform()) : null;
  });

  function iconOf(tab: TabRecord) {
    const name =
      ctx.registry.module(tab.module)?.rail?.icon ??
      ctx.registry.tabType(tab.tabType)?.icon ??
      "";
    return railIcon(name);
  }

  function badgeOf(tab: TabRecord) {
    return ctx.registry.tabType(tab.tabType)?.badge?.(tab.params) ?? null;
  }

  function close(tab: TabRecord): void {
    ctx.tabs.close(tab.id).catch((e: unknown) => toasts.show(errorMessage(e)));
  }

  function focusTab(list: Element | null, id: string): void {
    list
      ?.querySelector<HTMLElement>(
        `[data-tab="${CSS.escape(id)}"] [role="tab"]`,
      )
      ?.focus();
  }

  function onkeydown(e: KeyboardEvent, index: number): void {
    const to: Record<string, number> = {
      ArrowLeft: index - 1,
      ArrowRight: index + 1,
      Home: 0,
      End: tabs.length - 1,
    };
    const target = tabs[to[e.key] ?? -1];
    if (!target) return;
    e.preventDefault();
    ctx.tabs.activate(target.id);
    focusTab(
      (e.currentTarget as Element).closest('[role="tablist"]'),
      target.id,
    );
  }

  // Reordering by drag: pointer events, no library. The pointer is only
  // captured once a drag starts: Chromium (WebView2) sends the click to the
  // capturing element, so capturing on press would stop the tab's click.
  const DRAG_START = 4;
  let press: { id: string; x: number; dragging: boolean } | null = null;
  let dragging = $state<string | null>(null);

  function onpointerdown(e: PointerEvent, tab: TabRecord): void {
    if (e.button !== 0 || (e.target as Element).closest(".close")) return;
    press = { id: tab.id, x: e.clientX, dragging: false };
  }

  function onpointermove(e: PointerEvent): void {
    if (!press) return;
    if (!press.dragging) {
      if (Math.abs(e.clientX - press.x) < DRAG_START) return;
      press.dragging = true;
      (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    }
    dragging = press.id;
  }

  function dropIndex(list: Element | null, x: number, id: string): number {
    const others = [
      ...(list?.querySelectorAll<HTMLElement>("[data-tab]") ?? []),
    ].filter((el) => el.dataset.tab !== id);
    const index = others.findIndex((el) => {
      const r = el.getBoundingClientRect();
      return x < r.left + r.width / 2;
    });
    return index < 0 ? others.length : index;
  }

  // The bar scrolls sideways when the tabs do not fit: the active tab is
  // kept in view (an attachment on it), and a vertical wheel scrolls it.
  function keepInView(tab: HTMLElement): void {
    tab.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  function onwheel(e: WheelEvent): void {
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
    (e.currentTarget as HTMLElement).scrollLeft += e.deltaY;
  }

  function onpointerup(e: PointerEvent): void {
    if (press?.dragging)
      ctx.tabs.move(
        press.id,
        dropIndex(
          (e.currentTarget as Element).parentElement,
          e.clientX,
          press.id,
        ),
      );
    press = null;
    dragging = null;
  }
</script>

{#if tabs.length > 0}
  <div class="tabbar" role="tablist" aria-label="Open tabs" {onwheel}>
    {#each tabs as tab, index (tab.id)}
      {@const Icon = iconOf(tab)}
      {@const active = tab.id === activeId}
      {@const badge = badgeOf(tab)}
      {@const env = ctx.tabs.environmentOf(tab, ctx.project.environment)}
      {@const follows =
        ctx.registry.tabType(tab.tabType)?.environment === "window"}
      {@const production = follows && !tab.environment && !!env?.production}
      <div
        class="tab"
        data-tab={tab.id}
        data-active={active}
        data-dragging={dragging === tab.id}
        data-production={production ? "" : undefined}
        role="presentation"
        {@attach active && keepInView}
        onpointerdown={(e) => onpointerdown(e, tab)}
        {onpointermove}
        {onpointerup}
        onpointercancel={onpointerup}
      >
        <button
          type="button"
          role="tab"
          aria-selected={active}
          tabindex={active || (!activeId && index === 0) ? 0 : -1}
          onclick={() => ctx.tabs.activate(tab.id)}
          onkeydown={(e) => onkeydown(e, index)}
        >
          {#if badge}
            <span class="badge" style:color={badge.color}>{badge.text}</span>
          {:else if Icon}<Icon size={16} strokeWidth={1.5} />{/if}
          <span class="title">{tab.title}</span>{#if production}<span
              class="visually-hidden">, production</span
            >{/if}
          {#if env && !follows}
            <span
              class={["tab-env", { "env-prod": env.production }]}
              data-env={env.name}>{env.name}</span
            >
          {/if}
          {#if tab.dirty}
            <span class="dirty" role="img" aria-label="Unsaved changes"></span>
          {/if}
        </button>
        <button
          type="button"
          class="close"
          tabindex="-1"
          aria-label="Close {tab.title}"
          title={closeChord
            ? `Close ${tab.title}, ${closeChord}`
            : `Close ${tab.title}`}
          onclick={() => close(tab)}
        >
          <X size={14} strokeWidth={1.5} />
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tabbar {
    height: var(--h-tabs);
    flex: none;
    display: flex;
    align-items: stretch;
    background: var(--bg-0);
    box-shadow: inset 0 -1px 0 var(--line);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: var(--s-1);
    padding: 0 6px 0 var(--s-3);
    border-right: 1px solid var(--line);
    color: var(--text-2);
    flex: none;
    max-width: 240px;
    touch-action: none;
  }

  .tab[data-production] {
    box-shadow: inset 0 2px 0 var(--negative);
  }

  .tab[data-active="true"] {
    background: var(--bg-1);
    color: var(--text-1);
  }

  .tab[data-dragging="true"] {
    opacity: 0.6;
  }

  [role="tab"] {
    height: 100%;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    white-space: nowrap;
  }

  .badge {
    flex: none;
    font: 500 11px/1 var(--font-mono);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tab-env {
    margin-left: 2px;
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 400 11px/1 var(--font-mono);
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .env-prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .dirty {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 3px;
    background: var(--text-2);
  }

  .close {
    width: 24px;
    height: 24px;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--r-small);
    background: none;
    color: var(--text-3);
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text-1);
  }
</style>
