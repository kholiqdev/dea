// Moving tree rows with the pointer. Pointer events rather than HTML drag
// and drop, which WebDriver cannot drive in WebKitGTK and WebView2 (plan
// decision 5). Positions are pixels from the top of the list, so rows sit
// ROW_HEIGHT apart whatever the layout engine.

import {
  isWithin,
  parentOf,
  ROOT,
  ROW_HEIGHT,
  type RequestTreeState,
  type TreeRowData,
} from "./tree.svelte";

/** How far the pointer moves before a press becomes a drag, in pixels. */
export const DRAG_THRESHOLD = 4;

/** How long a closed folder stays under the pointer before it opens. */
export const OPEN_DELAY = 600;

/** Where a dragged item would land. */
export type Drop = {
  folder: string;
  /** Its place in `folder`, counted after it left its old place. */
  index: number;
  /** An insert line above row `at` (`rows.length` for the end), indented to `depth`. */
  line?: { at: number; depth: number };
  /** The row of the folder it would go to the end of. */
  into?: number;
};

type Tree = Pick<RequestTreeState, "itemsOf">;

/**
 * Where `source` lands when dropped `y` pixels from the top of the list:
 * the top or bottom of a row puts it before or after that row, the middle
 * of a folder row at the end of that folder. `null` where it cannot go or
 * would stay where it is.
 */
export function dropAt(
  tree: Tree,
  rows: readonly TreeRowData[],
  source: TreeRowData,
  y: number,
): Drop | null {
  const at = Math.floor(y / ROW_HEIGHT);
  const row = rows[at];
  if (y < 0 || !row) return null;
  const offset = y - at * ROW_HEIGHT;
  const quarter = ROW_HEIGHT / 4;
  let drop: Drop;
  if (row.kind === "folder" && offset >= quarter && offset < 3 * quarter) {
    drop = { folder: row.path, index: size(tree, row.path), into: at };
  } else if (offset < ROW_HEIGHT / 2) {
    const folder = parentOf(row.path);
    drop = {
      folder,
      index: place(tree, folder, row.path),
      line: { at, depth: row.depth },
    };
  } else if (row.kind === "folder" && (rows[at + 1]?.depth ?? 0) > row.depth) {
    // Below an open folder's row is the first place inside it.
    drop = {
      folder: row.path,
      index: 0,
      line: { at: at + 1, depth: row.depth + 1 },
    };
  } else {
    const folder = parentOf(row.path);
    drop = {
      folder,
      index: place(tree, folder, row.path) + 1,
      line: { at: at + 1, depth: row.depth },
    };
  }
  if (isWithin(drop.folder, source.path)) return null;
  if (source.kind === "request" && drop.folder === ROOT) return null;
  // The index counts the folder without the item.
  if (drop.folder === parentOf(source.path)) {
    const from = place(tree, drop.folder, source.path);
    if (from < drop.index) drop.index -= 1;
    if (from === drop.index) return null;
  }
  return drop;
}

function size(tree: Tree, folder: string): number {
  return tree.itemsOf(folder)?.length ?? 0;
}

function place(tree: Tree, folder: string, path: string): number {
  return tree.itemsOf(folder)?.findIndex((n) => n.path === path) ?? -1;
}

/** One press on a row that may become a drag. */
export class TreeDrag {
  /** The row being dragged, once the pointer has moved far enough. */
  source = $state.raw<TreeRowData | null>(null);
  drop = $state.raw<Drop | null>(null);

  #tree: Pick<RequestTreeState, "itemsOf" | "expanded">;
  #start: { row: TreeRowData; x: number; y: number } | null = null;
  #waiting: string | null = null;
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(tree: Pick<RequestTreeState, "itemsOf" | "expanded">) {
    this.#tree = tree;
  }

  /** Whether a press is going on, dragging or not yet. */
  get pressed(): boolean {
    return this.#start !== null;
  }

  press(row: TreeRowData, x: number, y: number): void {
    this.cancel();
    this.#start = { row, x, y };
  }

  /** The pointer is at `x`, `y` of the list, over `rows`. */
  point(x: number, y: number, rows: readonly TreeRowData[]): void {
    const start = this.#start;
    if (!start) return;
    if (!this.source) {
      if (Math.hypot(x - start.x, y - start.y) < DRAG_THRESHOLD) return;
      this.source = start.row;
    }
    this.#wait(rows[Math.floor(y / ROW_HEIGHT)], this.source);
    this.drop = dropAt(this.#tree, rows, this.source, y);
  }

  /** The button went up: what to move where, if the press was a drag. */
  release(): { path: string; folder: string; index: number } | null {
    const out =
      this.source && this.drop
        ? {
            path: this.source.path,
            folder: this.drop.folder,
            index: this.drop.index,
          }
        : null;
    this.cancel();
    return out;
  }

  cancel(): void {
    clearTimeout(this.#timer);
    this.#start = null;
    this.#waiting = null;
    this.source = null;
    this.drop = null;
  }

  /** A closed folder under the pointer opens once it stays there a while. */
  #wait(row: TreeRowData | undefined, source: TreeRowData): void {
    const folder =
      row?.kind === "folder" && !row.open && !isWithin(row.path, source.path)
        ? row.path
        : null;
    if (folder === this.#waiting) return;
    clearTimeout(this.#timer);
    this.#waiting = folder;
    if (folder)
      this.#timer = setTimeout(() => {
        this.#tree.expanded.add(folder);
        this.#waiting = null;
      }, OPEN_DELAY);
  }
}
