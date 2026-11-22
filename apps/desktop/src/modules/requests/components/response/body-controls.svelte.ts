import type { Filtered } from "$lib/ipc/bindings/Filtered";
import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
import type { BodyView } from "./body-view";

/**
 * The views the body of `response` offers, the one it opens on first. A
 * large body, a stream, and a body with no text have a view of their own
 * and offer none.
 */
function viewsOf(response: ResponseInfo): BodyView[] {
  switch (response.kind) {
    case "json":
    case "xml":
      return response.large ? [] : ["pretty", "raw"];
    case "html":
      return response.large ? [] : ["pretty", "raw", "preview"];
    case "text":
      return response.large ? [] : ["raw"];
    case "image":
    case "pdf":
      return ["preview"];
    default:
      return [];
  }
}

/**
 * How the body of one response is shown: its view and its JSONPath filter.
 * The second row of the response header sets them and the body reads them;
 * a new response gets new controls.
 */
export class BodyControls {
  /** The views the body offers; a choice shows when there is more than one. */
  readonly views: BodyView[];
  /** Whether the body can be filtered with a JSONPath. */
  readonly filterable: boolean;
  /** The JSONPath typed in the filter, kept while the filter is not shown. */
  filterPath = $state("");
  /** Whether the filter's field has the focus. */
  filterFocused = $state(false);
  /** What the filter selects: null while it is empty, not valid or not shown. */
  filtered = $state.raw<Filtered | null>(null);
  /** Why the filter selects nothing, when it is not valid or could not run. */
  filterError = $state<string | null>(null);

  #chosen = $state<BodyView | null>(null);

  constructor(response: ResponseInfo) {
    this.views = viewsOf(response);
    this.filterable = response.kind === "json" && !response.large;
  }

  /**
   * Whether the filter is in use: focused, or with a path. Its result shows
   * whatever the view, so the choice of view steps aside.
   */
  get filterActive(): boolean {
    return this.filterFocused || this.filterPath.trim() !== "";
  }

  /** The view shown: the one chosen, or else the first the body offers. */
  get view(): BodyView {
    const chosen = this.#chosen;
    return chosen !== null && this.views.includes(chosen)
      ? chosen
      : (this.views[0] ?? "raw");
  }

  set view(view: BodyView) {
    this.#chosen = view;
  }
}
