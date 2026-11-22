// The second row of a JSON response's header: the sections, the choice of
// view and the JSONPath filter, which is always there. The field's widths
// are drawn by the CSS of `JsonFilter`; the numbers here follow it.

/** The field at rest, with room for the placeholder and the keycaps. */
export const FILTER_IDLE_WIDTH = 168;
/** The field in use, where the row has the room. */
export const FILTER_ACTIVE_WIDTH = 280;
/** The narrowest the field gets before the sections give way. */
export const FILTER_MIN_WIDTH = 120;
/** The default chord of `requests.focusFilter`, drawn as the field's keycaps. */
export const FILTER_CHORD = "Mod+Alt+F";
/** Used until the buttons or the menu were measured once. */
export const VIEW_BUTTONS_FALLBACK = 92;
export const VIEW_MENU_FALLBACK = 71;

/** The row's padding, 16 px on each side. */
const PADDING = 2 * 16;
/** The gaps between the sections, the choice of view and the field. */
const GAPS = 2 * 12;

export interface RowWidths {
  /** The response pane; 0 while not measured. */
  pane: number;
  /** The tablist's natural width (its scrollWidth). */
  sections: number;
  /** The Pretty / Raw buttons. */
  buttons: number;
  /** The menu button. */
  dropdown: number;
}

export interface FilterLayout {
  views: "buttons" | "dropdown" | "hidden";
  keys: boolean;
}

/**
 * How the row is laid out. A filter in use (focused, or with a path) shows
 * its result as formatted JSON whatever the view, so the choice of view
 * gives it its room. At rest, the buttons show while they fit beside the
 * field at its full width, then a menu; the keycaps show only where the
 * field gets that width.
 */
export function filterLayout(widths: RowWidths, active: boolean): FilterLayout {
  if (active) return { views: "hidden", keys: false };
  if (widths.pane === 0) return { views: "buttons", keys: true };
  const room = widths.pane - PADDING - widths.sections - GAPS;
  if (room - widths.buttons >= FILTER_IDLE_WIDTH)
    return { views: "buttons", keys: true };
  return {
    views: "dropdown",
    keys: room - widths.dropdown >= FILTER_IDLE_WIDTH,
  };
}
