import { Compartment, Facet, RangeSetBuilder } from "@codemirror/state";
import type { Extension } from "@codemirror/state";
import {
  Decoration,
  ViewPlugin,
  hoverTooltip,
  type DecorationSet,
  type EditorView,
  type ViewUpdate,
} from "@codemirror/view";
import { variableRanges, type VariableState } from "./variables";

const variableState = Facet.define<
  VariableState | undefined,
  VariableState | undefined
>({
  combine: (values) => values[0],
});

const defined = Decoration.mark({ class: "cm-var" });
const undefinedMark = Decoration.mark({ class: "cm-var-undefined" });

function decorate(view: EditorView): DecorationSet {
  const state = view.state.facet(variableState);
  const builder = new RangeSetBuilder<Decoration>();
  for (const { from, to, name } of variableRanges(view.state.doc.toString())) {
    builder.add(
      from,
      to,
      state && !state.defined(name) ? undefinedMark : defined,
    );
  }
  return builder.finish();
}

const highlight = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(update: ViewUpdate) {
      if (
        update.docChanged ||
        update.startState.facet(variableState) !==
          update.state.facet(variableState)
      ) {
        this.decorations = decorate(update.view);
      }
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

const hover = hoverTooltip((view, pos) => {
  const state = view.state.facet(variableState);
  if (!state) return null;
  const range = variableRanges(view.state.doc.toString()).find(
    (r) => pos >= r.from && pos <= r.to,
  );
  if (!range) return null;
  const info = state.describe(range.name);
  return {
    pos: range.from,
    end: range.to,
    above: true,
    create() {
      const dom = document.createElement("div");
      dom.className = "cm-var-tooltip";
      const name = document.createElement("div");
      name.className = "cm-var-tooltip-name";
      name.textContent = range.name;
      dom.append(name);
      const detail = document.createElement("div");
      detail.className = "cm-var-tooltip-detail";
      detail.textContent = info
        ? `${info.shown} (${info.source})`
        : "Not defined";
      dom.append(detail);
      return { dom };
    },
  };
});

export const variableCompartment = new Compartment();

/** Highlight and hover support for `{{variable}}`; swap the state through the compartment. */
export function variableExtension(state: VariableState | undefined): Extension {
  return [variableCompartment.of(variableState.of(state)), highlight, hover];
}

export function variableStateExtension(
  state: VariableState | undefined,
): Extension {
  return variableState.of(state);
}
