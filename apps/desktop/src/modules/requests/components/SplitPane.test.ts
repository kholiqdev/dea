import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import { expect, onTestFinished, test, vi } from "vitest";
import type { ResponseLayout } from "$lib/ipc/bindings/ResponseLayout";
import type { ResponseSplit } from "$lib/ipc/bindings/ResponseSplit";
import { fakeResizeObserver } from "../test-support";
import SplitPane from "./SplitPane.svelte";

const text = (t: string) =>
  createRawSnippet(() => ({ render: () => `<p>${t}</p>` }));

type Commit = (layout: ResponseLayout, percent: number) => unknown;

/** The split at `width` × `height` px, at the left top of the page. */
function renderSplit(
  opts: {
    layout?: ResponseLayout;
    split?: ResponseSplit;
    width?: number;
    height?: number;
    oncommit?: Commit;
  } = {},
) {
  const observer = fakeResizeObserver();
  onTestFinished(observer.restore);
  const width = opts.width ?? 1000;
  const height = opts.height ?? 600;
  const { container } = render(SplitPane, {
    props: {
      layout: opts.layout ?? "right",
      split: opts.split ?? { right: 44, below: 40 },
      oncommit: (opts.oncommit ?? vi.fn()) as (
        layout: ResponseLayout,
        percent: number,
      ) => void,
      editor: text("Request"),
      response: text("Response"),
    },
  });
  const split = container.querySelector<HTMLElement>(".split")!;
  split.getBoundingClientRect = () => new DOMRect(0, 0, width, height);
  observer.resize(split, width, height);
  const divider = screen.getByRole("separator", {
    name: "Resize the request and response",
  });
  const editor = split.querySelector<HTMLElement>(":scope > .editor")!;
  return { split, divider, editor };
}

/** Drags the divider to a point, letting go there. */
async function drag(
  divider: HTMLElement,
  to: { toX?: number; toY?: number },
): Promise<void> {
  const at = { clientX: to.toX ?? 0, clientY: to.toY ?? 0 };
  await fireEvent.pointerDown(divider, { pointerId: 1, button: 0 });
  await fireEvent.pointerMove(divider, { pointerId: 1, ...at });
  await fireEvent.pointerUp(divider, { pointerId: 1, ...at });
}

test("the_split_has_the_editor_the_divider_and_the_response", () => {
  const { split } = renderSplit();
  expect(split).toHaveAttribute("data-layout", "right");
  expect([...split.children].map((c) => c.className.split(" ")[0])).toEqual([
    "editor",
    "divider",
    "response",
  ]);
  expect(split.querySelector(".editor")).toHaveTextContent("Request");
  expect(split.querySelector(".response")).toHaveTextContent("Response");
});

test("the_divider_reports_its_value", () => {
  const { divider, editor } = renderSplit();
  expect(divider).toHaveAttribute("aria-valuenow", "44");
  // 360 px of 1000 for the request, 400 px for the response.
  expect(divider).toHaveAttribute("aria-valuemin", "36");
  expect(divider).toHaveAttribute("aria-valuemax", "60");
  expect(divider).toHaveAttribute("aria-orientation", "vertical");
  expect(divider).toHaveAttribute("tabindex", "0");
  expect(editor.style.flexBasis).toBe("44%");
});

test("dragging_past_the_minimum_stops_at_it", async () => {
  const oncommit = vi.fn();
  const { divider } = renderSplit({ layout: "right", width: 1000, oncommit });
  await drag(divider, { toX: 100 });
  expect(oncommit).toHaveBeenLastCalledWith("right", 36); // 360 px of 1000 px
  await drag(divider, { toX: 950 });
  expect(oncommit).toHaveBeenLastCalledWith("right", 60); // response 400 px
  expect(oncommit).toHaveBeenCalledTimes(2);
});

test("a_drag_resizes_at_once_and_saves_when_let_go", async () => {
  const oncommit = vi.fn();
  const { divider, editor } = renderSplit({ oncommit });
  await fireEvent.pointerDown(divider, { pointerId: 1, button: 0 });
  await fireEvent.pointerMove(divider, { pointerId: 1, clientX: 503 });
  expect(editor.style.flexBasis).toBe("50%");
  expect(divider).toHaveAttribute("aria-valuenow", "50");
  await fireEvent.pointerMove(divider, { pointerId: 1, clientX: 520 });
  expect(oncommit).not.toHaveBeenCalled();
  await fireEvent.pointerUp(divider, { pointerId: 1, clientX: 520 });
  expect(oncommit).toHaveBeenCalledOnce();
  expect(oncommit).toHaveBeenCalledWith("right", 52);
});

test("a_click_without_a_move_saves_nothing", async () => {
  const oncommit = vi.fn();
  const { divider } = renderSplit({ oncommit });
  await fireEvent.pointerDown(divider, { pointerId: 1, button: 0 });
  await fireEvent.pointerUp(divider, { pointerId: 1 });
  expect(oncommit).not.toHaveBeenCalled();
});

test("arrow_keys_resize_by_two_percent", async () => {
  const oncommit = vi.fn();
  const { divider } = renderSplit({ oncommit });
  divider.focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(oncommit).toHaveBeenLastCalledWith("right", 46);
  await userEvent.keyboard("{ArrowLeft}");
  expect(oncommit).toHaveBeenLastCalledWith("right", 42);
  await userEvent.keyboard("{Home}");
  expect(oncommit).toHaveBeenLastCalledWith("right", 36);
  await userEvent.keyboard("{End}");
  expect(oncommit).toHaveBeenLastCalledWith("right", 60);
  // Up and down belong to a divider between panes on top of each other.
  oncommit.mockClear();
  await userEvent.keyboard("{ArrowUp}{ArrowDown}");
  expect(oncommit).not.toHaveBeenCalled();
});

test("double_click_restores_the_default", async () => {
  const oncommit = vi.fn();
  const { divider } = renderSplit({
    split: { right: 60, below: 40 },
    oncommit,
  });
  await userEvent.dblClick(divider);
  expect(oncommit).toHaveBeenCalledOnce();
  expect(oncommit).toHaveBeenLastCalledWith("right", 44);
  oncommit.mockClear();
  divider.focus();
  await userEvent.keyboard("{Enter}");
  expect(oncommit).toHaveBeenCalledOnce();
  expect(oncommit).toHaveBeenLastCalledWith("right", 44);
});

test("a_narrow_work_area_stacks_the_panes", async () => {
  const oncommit = vi.fn();
  const { split, divider, editor } = renderSplit({
    layout: "right",
    width: 700,
    split: { right: 44, below: 30 },
    oncommit,
  });
  expect(split).toHaveAttribute("data-layout", "below");
  expect(divider).toHaveAttribute("aria-orientation", "horizontal");
  expect(divider).toHaveAttribute("aria-valuenow", "30");
  expect(editor.style.flexBasis).toBe("30%");
  divider.focus();
  await userEvent.keyboard("{ArrowDown}");
  expect(oncommit).toHaveBeenLastCalledWith("below", 32);
});

test("the_below_layout_takes_its_minimums_from_the_height", async () => {
  const oncommit = vi.fn();
  const { divider } = renderSplit({
    layout: "below",
    width: 1000,
    height: 800,
    oncommit,
  });
  expect(divider).toHaveAttribute("aria-orientation", "horizontal");
  // 120 px of 800 for the request, 160 px for the response.
  expect(divider).toHaveAttribute("aria-valuemin", "20");
  expect(divider).toHaveAttribute("aria-valuemax", "80");
  await drag(divider, { toY: 720 });
  expect(oncommit).toHaveBeenLastCalledWith("below", 80);
});

test("minimums_that_do_not_fit_leave_only_the_range", () => {
  // 120 + 160 px do not fit in 250 px.
  const { divider } = renderSplit({ layout: "below", height: 250 });
  expect(divider).toHaveAttribute("aria-valuemin", "20");
  expect(divider).toHaveAttribute("aria-valuemax", "80");
});

test("minimums_that_fit_but_round_across_keep_one_share", async () => {
  // 360 + 400 px fit in 761 px, but 360 px rounds up to 48% and the 400 px
  // left for the response round down to 47%.
  const oncommit = vi.fn();
  const { divider, editor } = renderSplit({
    layout: "right",
    width: 761,
    oncommit,
  });
  expect(divider).toHaveAttribute("aria-valuemin", "47");
  expect(divider).toHaveAttribute("aria-valuemax", "47");
  expect(divider).toHaveAttribute("aria-valuenow", "47");
  expect(editor.style.flexBasis).toBe("47%");
  await drag(divider, { toX: 700 });
  expect(oncommit).toHaveBeenLastCalledWith("right", 47);
  await drag(divider, { toX: 100 });
  expect(oncommit).toHaveBeenLastCalledWith("right", 47);
});

test("a_saved_share_outside_the_minimums_shows_within_them", () => {
  const { divider, editor } = renderSplit({ split: { right: 75, below: 40 } });
  expect(divider).toHaveAttribute("aria-valuenow", "60");
  expect(editor.style.flexBasis).toBe("60%");
});

test("keys_pressed_while_saving_build_on_the_unsaved_share", async () => {
  let finish = () => {};
  const oncommit = vi.fn(
    () => new Promise<void>((resolve) => (finish = resolve)),
  );
  const { divider } = renderSplit({ oncommit });
  divider.focus();
  await userEvent.keyboard("{ArrowRight}{ArrowRight}");
  expect(oncommit.mock.calls).toEqual([
    ["right", 46],
    ["right", 48],
  ]);
  expect(divider).toHaveAttribute("aria-valuenow", "48");
  // Settled, and the saved share (the prop, unchanged here) shows again.
  finish();
  await vi.waitFor(() =>
    expect(divider).toHaveAttribute("aria-valuenow", "44"),
  );
});
