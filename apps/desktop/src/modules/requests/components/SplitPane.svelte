<!--
  The request beside or above its response, with a divider between them that
  a drag or the keyboard moves. The share comes from the caller and goes back
  to it through `oncommit`; while a drag runs, or until a commit settles, the
  divider shows the new share. A work area under 760 px wide always stacks.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ResponseLayout } from "$lib/ipc/bindings/ResponseLayout";
  import type { ResponseSplit } from "$lib/ipc/bindings/ResponseSplit";
  import { SPLIT_DEFAULT, SPLIT_MAX, SPLIT_MIN } from "../state/layout";

  let {
    layout,
    split,
    oncommit,
    stacked = $bindable(false),
    editor,
    response,
  }: {
    layout: ResponseLayout;
    split: ResponseSplit;
    /**
     * Called once a drag ends, and on each key. A returned promise keeps
     * the new share shown until it settles.
     */
    oncommit: (layout: ResponseLayout, percent: number) => void | Promise<void>;
    /** True while the work area is too narrow to put the response beside. */
    stacked?: boolean;
    editor: Snippet;
    response: Snippet;
  } = $props();

  const STACK_UNDER = 760;
  const STEP = 2;
  /** The least each panel keeps, in px along the split. */
  const LEAST: Record<ResponseLayout, { editor: number; response: number }> = {
    right: { editor: 360, response: 400 },
    below: { editor: 120, response: 160 },
  };

  let root = $state<HTMLElement>();
  let size = $state.raw({ width: 0, height: 0 });
  // A share not saved yet: dragged, or sent to `oncommit` and not settled.
  let live = $state.raw<{ layout: ResponseLayout; percent: number } | null>(
    null,
  );
  let drag = $state.raw<{ moved: boolean } | null>(null);

  const effective = $derived<ResponseLayout>(stacked ? "below" : layout);
  const bounds = $derived(
    limits(effective, effective === "right" ? size.width : size.height),
  );
  const percent = $derived(
    clamp(live?.layout === effective ? live.percent : split[effective], bounds),
  );

  /**
   * The range the request's share may take: 20–80%, narrowed so that each
   * panel keeps its least size, unless both together do not fit. When they
   * fit but no whole percent keeps both, the share is the one that keeps
   * the response's.
   */
  function limits(
    at: ResponseLayout,
    total: number,
  ): { min: number; max: number } {
    const range = { min: SPLIT_MIN, max: SPLIT_MAX };
    if (total <= 0) return range;
    const least = LEAST[at];
    if (least.editor + least.response > total) return range;
    const min = Math.max(SPLIT_MIN, Math.ceil((least.editor * 100) / total));
    const max = Math.min(
      SPLIT_MAX,
      Math.floor(((total - least.response) * 100) / total),
    );
    return min <= max ? { min, max } : { min: max, max };
  }

  function clamp(value: number, range: { min: number; max: number }): number {
    return Math.min(range.max, Math.max(range.min, Math.round(value)));
  }

  // jsdom has no ResizeObserver; the size then stays unknown and the split
  // keeps the layout it is given.
  function observe(el: HTMLElement): (() => void) | void {
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      // Not laid out, as in a hidden view: nothing to go by.
      if (width === 0 && height === 0) return;
      size = { width, height };
      stacked = width < STACK_UNDER;
    });
    observer.observe(el);
    return () => observer.disconnect();
  }

  function commit(value: number): void {
    const at = effective;
    const pending = { layout: at, percent: value };
    live = pending;
    // A later drag or key replaces `live`; only this commit's share goes.
    void Promise.resolve(oncommit(at, value)).finally(() => {
      if (live === pending) live = null;
    });
  }

  function onpointerdown(e: PointerEvent): void {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    drag = { moved: false };
  }

  function onpointermove(e: PointerEvent): void {
    if (!drag || !root) return;
    const box = root.getBoundingClientRect();
    const [at, total] =
      effective === "right"
        ? [e.clientX - box.left, box.width]
        : [e.clientY - box.top, box.height];
    if (total <= 0) return;
    if (!drag.moved) drag = { moved: true };
    live = { layout: effective, percent: clamp((at * 100) / total, bounds) };
  }

  function onpointerup(): void {
    const ended = drag;
    drag = null;
    // Not if the split stacked meanwhile: the share is for the other axis.
    if (ended?.moved && live?.layout === effective) commit(live.percent);
  }

  function onkeydown(e: KeyboardEvent): void {
    const [less, more] =
      effective === "right"
        ? ["ArrowLeft", "ArrowRight"]
        : ["ArrowUp", "ArrowDown"];
    const to: Record<string, number> = {
      [less]: clamp(percent - STEP, bounds),
      [more]: clamp(percent + STEP, bounds),
      Home: bounds.min,
      End: bounds.max,
      Enter: SPLIT_DEFAULT[effective],
    };
    const value = to[e.key];
    if (value === undefined) return;
    e.preventDefault();
    if (value !== percent) commit(value);
  }

  function ondblclick(): void {
    if (SPLIT_DEFAULT[effective] !== percent) commit(SPLIT_DEFAULT[effective]);
  }
</script>

<div
  class={["split", { dragging: drag !== null }]}
  data-layout={effective}
  bind:this={root}
  {@attach observe}
>
  <div class="editor" style:flex-basis="{percent}%">
    {@render editor()}
  </div>
  <!-- A focusable window splitter: arrows, Home, End and Enter move it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="divider"
    role="separator"
    tabindex="0"
    aria-label="Resize the request and response"
    aria-orientation={effective === "right" ? "vertical" : "horizontal"}
    aria-valuenow={percent}
    aria-valuemin={bounds.min}
    aria-valuemax={bounds.max}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    {onkeydown}
    {ondblclick}
  ></div>
  <div class="response">
    {@render response()}
  </div>
</div>

<style>
  .split {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }

  .split[data-layout="below"] {
    flex-direction: column;
  }

  .split.dragging {
    user-select: none;
  }

  .editor,
  .response {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .editor {
    flex-grow: 0;
    flex-shrink: 0;
  }

  .response {
    flex: 1 1 0;
  }

  /* A 1 px line with 8 px to take hold of. */
  .divider {
    position: relative;
    z-index: 1;
    flex: none;
    background: var(--line);
  }

  .split[data-layout="right"] > .divider {
    width: 1px;
    cursor: col-resize;
  }

  .split[data-layout="below"] > .divider {
    height: 1px;
    cursor: row-resize;
  }

  .divider::before {
    content: "";
    position: absolute;
  }

  .split[data-layout="right"] > .divider::before {
    inset: 0 -3px 0 -4px;
  }

  .split[data-layout="below"] > .divider::before {
    inset: -4px 0 -3px;
  }

  /* The grip shows where the divider can be taken. */
  .divider::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 50%;
    width: 4px;
    height: 24px;
    border-radius: 2px;
    background: var(--text-3);
    transform: translate(-50%, -50%);
    opacity: 0;
    transition: opacity 120ms ease-out;
    pointer-events: none;
  }

  .split[data-layout="below"] > .divider::after {
    width: 24px;
    height: 4px;
  }

  .divider:hover::after,
  .divider:focus-visible::after,
  .split.dragging > .divider::after {
    opacity: 1;
  }
</style>
