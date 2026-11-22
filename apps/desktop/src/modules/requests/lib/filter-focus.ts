// How the shortcut for the filter reaches a response pane: each pane lets
// its tab's id take the focus, and the intent asks by that id.

const panes = new Map<string, () => Promise<boolean>>();

/** Lets the response pane of `tabId` take the focus for its filter; returns the undo. */
export function provideFilterFocus(
  tabId: string,
  focus: () => Promise<boolean>,
): () => void {
  panes.set(tabId, focus);
  return () => {
    // A pane that mounted again may have registered before this cleanup.
    if (panes.get(tabId) === focus) panes.delete(tabId);
  };
}

/** Focuses the filter of `tabId`; false without a pane or a JSON body to filter. */
export async function focusFilter(tabId: string): Promise<boolean> {
  return (await panes.get(tabId)?.()) ?? false;
}
