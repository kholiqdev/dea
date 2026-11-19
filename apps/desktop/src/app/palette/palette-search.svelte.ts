// One command palette session: the items gathered when it opened, the
// query, and the active row (F1-FON-11).

import { rank } from "$lib/search/fuzzy";
import type { Registry } from "../contributions/registry.svelte";
import type { Action, PaletteItem } from "../contributions/types";

/** The command that opens the palette; it is not listed in it. */
export const PALETTE_COMMAND = "shell.palette";

/** Items per group while the query is empty. */
const PER_GROUP = 5;
/** Matches shown at most while typing. */
const MAX_RESULTS = 50;
const SETTINGS_ORDER = 80;
const COMMANDS_ORDER = 90;

/** A palette item with the group it shows under. */
export interface PaletteEntry extends PaletteItem {
  /** Unique across the palette. */
  key: string;
  group: string;
  order: number;
}

export interface PaletteGroup {
  title: string;
  entries: PaletteEntry[];
}

/** `entries` grouped by group order; within a group, in the given order. */
function grouped(entries: PaletteEntry[]): PaletteGroup[] {
  const groups: (PaletteGroup & { order: number })[] = [];
  for (const entry of entries) {
    let group = groups.find((g) => g.title === entry.group);
    if (!group) {
      group = { title: entry.group, order: entry.order, entries: [] };
      groups.push(group);
    }
    group.entries.push(entry);
  }
  // Stable: groups with the same order keep the order they appeared in.
  return groups
    .sort((a, b) => a.order - b.order)
    .map(({ title, entries: e }) => ({ title, entries: e }));
}

export class PaletteSearch {
  query = $state("");
  #entries = $state.raw<PaletteEntry[]>([]);
  /** The row the person moved to; null follows the best match. */
  #activeKey = $state<string | null>(null);

  /** Matches for the query, best first; null while the query is empty. */
  #ranked = $derived(
    this.query.trim() === ""
      ? null
      : rank(this.query, this.#entries, (e) => e.title, MAX_RESULTS),
  );

  groups = $derived.by((): PaletteGroup[] => {
    if (this.#ranked) return grouped(this.#ranked);
    return grouped(this.#entries).map((g) => ({
      title: g.title,
      entries: g.entries.slice(0, PER_GROUP),
    }));
  });

  /** The rows in the order they show. */
  rows = $derived(this.groups.flatMap((g) => g.entries));

  /**
   * The active row: the one the person moved to while it still shows,
   * else the best match, which may sit in a later group.
   */
  get active(): number {
    const key = this.#activeKey ?? this.#ranked?.[0]?.key;
    const index = key ? this.rows.findIndex((r) => r.key === key) : -1;
    return index === -1 ? 0 : index;
  }

  set active(index: number) {
    this.#activeKey = this.rows[index]?.key ?? null;
  }

  /** A new query: the active row follows the best match again. */
  followBest(): void {
    this.#activeKey = null;
  }

  get activeEntry(): PaletteEntry | undefined {
    return this.rows[this.active];
  }

  /**
   * Gathers commands and Settings sections at once, then each module's
   * items as its source answers. A source that fails is left out, and one
   * that never answers hides nothing else.
   */
  async collect(
    registry: Registry,
    run: (action: Action) => Promise<void>,
  ): Promise<void> {
    const commands: PaletteEntry[] = registry.commands
      .filter((c) => c.id !== PALETTE_COMMAND)
      .map((c) => ({
        key: `command:${c.id}`,
        id: c.id,
        title: c.title,
        command: c.id,
        group: "Commands",
        order: COMMANDS_ORDER,
        open: () => run(c.action),
      }));
    const opensSettings = registry.commands.some(
      (c) => c.id === "settings.open",
    );
    const sections: PaletteEntry[] = registry.settingsSections.map((s, i) => ({
      key: `settings:${s.id}`,
      id: s.id,
      title: s.title,
      // Mod+, opens Settings on its first section.
      ...(i === 0 && opensSettings ? { command: "settings.open" } : {}),
      group: "Settings",
      order: SETTINGS_ORDER,
      open: () => run({ type: "openSettings", section: s.id }),
    }));
    this.#entries = [...sections, ...commands];
    await Promise.all(
      registry.searchSources.map(async (s) => {
        try {
          const { default: source } = await s.source();
          const items = await source();
          // Module items come before the built-in ones, as their groups do.
          this.#entries = [
            ...items.map((item) => ({
              ...item,
              key: `${s.id}:${item.id}`,
              group: s.group,
              order: s.order,
            })),
            ...this.#entries,
          ];
        } catch {
          // Only the source's id: its items may name things a log should not.
          console.warn(`The palette source ${s.id} failed`);
        }
      }),
    );
  }

  /** Moves the active row by `delta`, wrapping around. */
  move(delta: number): void {
    const n = this.rows.length;
    if (n > 0) this.active = (this.active + delta + n) % n;
  }
}
