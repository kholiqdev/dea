<!--
  The command palette (F1-FON-11): one search over commands, Settings and
  what modules add. The keyboard stays in the search box; the rows follow
  the active one.
-->
<script lang="ts">
  import { tick } from "svelte";
  import Dialog from "@dea/ui/Dialog.svelte";
  import {
    chordFromEvent,
    currentPlatform,
    formatChordText,
    parseChord,
    sameChord,
  } from "$lib/keyboard/chord";
  import { settings } from "$lib/settings/settings.svelte";
  import { effectiveShortcuts } from "../contributions/shortcuts";
  import { shellContext } from "../shell/context";
  import { runAction } from "../shell/hotkeys.svelte";
  import { menus } from "../shell/menus.svelte";
  import { PALETTE_COMMAND, PaletteSearch } from "./palette-search.svelte";
  import PaletteRow from "./PaletteRow.svelte";

  const ctx = shellContext();
  const platform = currentPlatform();
  const search = new PaletteSearch();
  const uid = $props.id();
  const listId = `${uid}-list`;
  const optionId = (i: number) => `${uid}-option-${i}`;

  const chords = $derived(
    Object.fromEntries(
      effectiveShortcuts(ctx.registry, settings.shortcuts).map((b) => [
        b.command,
        b.chord,
      ]),
    ),
  );

  // Commands and Settings show at once; module items follow as each
  // source answers.
  performance.mark?.("palette-open");
  const collecting = search.collect(ctx.registry, (action) =>
    runAction(action, ctx),
  );
  void tick().then(() =>
    performance.measure?.("palette-first-result", "palette-open"),
  );
  void collecting
    .then(tick)
    .then(() => performance.measure?.("palette-all-results", "palette-open"));

  // Keep the active row visible as the keyboard moves it.
  $effect(() => {
    document
      .getElementById(optionId(search.active))
      ?.scrollIntoView?.({ block: "nearest" });
  });

  function close(): void {
    menus.palette = false;
  }

  function shortcutOf(command: string | undefined): string | null {
    const chord = command ? chords[command] : undefined;
    return chord ? formatChordText(chord, platform) : null;
  }

  async function choose(index: number, newTab: boolean): Promise<void> {
    const entry = search.rows[index];
    if (!entry) return;
    close();
    await tick();
    await entry.open(newTab && entry.opensTab ? "new" : "active");
  }

  function isPaletteChord(e: KeyboardEvent): boolean {
    const text = chords[PALETTE_COMMAND];
    const pressed = chordFromEvent(e, platform);
    if (!text || !pressed) return false;
    try {
      return sameChord(parseChord(text), pressed);
    } catch {
      return false;
    }
  }

  function onkeydown(e: KeyboardEvent): void {
    // Keys that pick characters in an input method are not the palette's.
    if (e.isComposing || e.keyCode === 229) return;
    const moves: Record<string, () => void> = {
      ArrowDown: () => search.move(1),
      ArrowUp: () => search.move(-1),
      Home: () => (search.active = 0),
      End: () => (search.active = search.rows.length - 1),
    };
    if (e.key in moves) {
      e.preventDefault();
      moves[e.key]?.();
    } else if (e.key === "Enter") {
      e.preventDefault();
      void choose(search.active, chordFromEvent(e, platform)?.mod === true);
    } else if (isPaletteChord(e)) {
      e.preventDefault();
      close();
    }
  }
</script>

<Dialog
  title="Search"
  hideTitle
  oncancel={close}
  --dialog-width="640px"
  --dialog-top="64px"
  --dialog-padding="var(--s-2)"
  --dialog-gap="var(--s-2)"
>
  <div class="field">
    <input
      class="query"
      type="text"
      role="combobox"
      aria-label="Search"
      aria-expanded="true"
      aria-controls={listId}
      aria-autocomplete="list"
      aria-activedescendant={search.rows.length > 0
        ? optionId(search.active)
        : undefined}
      placeholder="Search requests, tables, hosts, settings and commands"
      autocomplete="off"
      spellcheck="false"
      data-autofocus
      bind:value={search.query}
      oninput={() => search.followBest()}
      {onkeydown}
    />
    <kbd class="kc" aria-hidden="true">Esc</kbd>
  </div>
  <!-- A click in the list keeps the keyboard in the search box. -->
  <div
    class="list"
    id={listId}
    role="listbox"
    aria-label="Results"
    tabindex="-1"
    onmousedown={(e) => e.preventDefault()}
  >
    {#each search.groups as group (group.title)}
      <div class="group" role="group" aria-label={group.title}>
        <div class="side-label" aria-hidden="true">{group.title}</div>
        {#each group.entries as entry (entry.key)}
          {@const index = search.rows.indexOf(entry)}
          <PaletteRow
            {entry}
            id={optionId(index)}
            active={index === search.active}
            shortcut={shortcutOf(entry.command)}
            onchoose={() => void choose(index, false)}
            onpoint={() => (search.active = index)}
          />
        {/each}
      </div>
    {/each}
    {#if search.query.trim() !== "" && search.rows.length === 0}
      <p class="empty">No results for "{search.query.trim()}"</p>
    {/if}
  </div>
</Dialog>

<style>
  .field {
    height: 40px;
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-2) 0 var(--s-3);
    border-radius: var(--r-ctl);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .query {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: none;
    color: var(--text-1);
    font: inherit;
  }

  .query:focus {
    outline: none;
  }

  .list {
    max-height: min(60vh, 520px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .group {
    display: flex;
    flex-direction: column;
  }

  .side-label {
    padding: var(--s-1) var(--s-3);
    font-size: 12px;
    line-height: 16px;
    font-weight: 400;
    color: var(--text-3);
  }

  .empty {
    padding: var(--s-3);
    color: var(--text-3);
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
