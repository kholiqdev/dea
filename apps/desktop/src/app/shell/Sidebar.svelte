<script lang="ts">
  import { innerWidth } from "svelte/reactivity/window";
  import type { ModuleManifest } from "../contributions/types";
  import { shellContext } from "./context";
  import LazyView from "./LazyView.svelte";
  import { SIDEBAR_MIN, clampSidebarWidth } from "./shell-state.svelte";

  let { module }: { module: ModuleManifest } = $props();

  const ctx = shellContext();
  const STEP = 16;
  const windowWidth = $derived(innerWidth.current ?? 1280);
  const max = $derived(
    Math.max(SIDEBAR_MIN, windowWidth - ctx.shell.railWidth - 480),
  );
  const width = $derived(
    clampSidebarWidth(
      ctx.shell.sidebar(module.id).width,
      windowWidth,
      ctx.shell.railWidth,
    ),
  );

  function resize(px: number): void {
    ctx.shell.setSidebarWidth(
      module.id,
      clampSidebarWidth(px, windowWidth, ctx.shell.railWidth),
    );
  }

  let drag: { x: number; width: number } | null = null;

  function onpointerdown(e: PointerEvent): void {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    drag = { x: e.clientX, width };
  }

  function onpointermove(e: PointerEvent): void {
    if (drag) resize(drag.width + e.clientX - drag.x);
  }

  function onpointerup(): void {
    drag = null;
  }

  function onkeydown(e: KeyboardEvent): void {
    const to: Record<string, number> = {
      ArrowLeft: width - STEP,
      ArrowRight: width + STEP,
      Home: SIDEBAR_MIN,
      End: max,
    };
    const px = to[e.key];
    if (px === undefined) return;
    e.preventDefault();
    resize(px);
  }
</script>

<aside class="side" style:width="{width}px" aria-label="{module.title} sidebar">
  <div class="head"><h2>{module.title}</h2></div>
  <div class="body">
    {#if module.sidebar}<LazyView loader={module.sidebar} />{/if}
  </div>
  <!-- A focusable window splitter: arrows, Home and End resize it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="resize"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize the sidebar"
    aria-valuenow={width}
    aria-valuemin={SIDEBAR_MIN}
    aria-valuemax={max}
    tabindex="0"
    data-shell-control
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    {onkeydown}
  ></div>
</aside>

<style>
  .side {
    position: relative;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-0);
    border-right: 1px solid var(--line);
  }

  .head {
    height: 44px;
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 var(--s-2) 0 var(--s-4);
  }

  h2 {
    font-size: 13px;
    font-weight: 500;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--s-1) var(--s-2) var(--s-4);
  }

  .resize {
    position: absolute;
    top: 0;
    right: -3px;
    width: 5px;
    height: 100%;
    cursor: col-resize;
    z-index: 2;
  }

  .resize:hover,
  .resize:focus-visible {
    background: var(--line-strong);
  }
</style>
