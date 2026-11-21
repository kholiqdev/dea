import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { setUpRequestTabs } from "../../request-tab-support.svelte";
import { fakeRequestsIpc, fakeResponse, fakeRows } from "../../test-support";
import LargeBody from "./LargeBody.svelte";

// Row requests that wait until the test lets them through.
const held = vi.hoisted(() => ({
  on: false,
  pending: [] as { from: number; settle: () => void }[],
}));
vi.mock("$lib/ipc/responses", async (original) => {
  const real = await original<typeof import("$lib/ipc/responses")>();
  return {
    ...real,
    responseRows: (id: string, from: number, count: number) =>
      held.on
        ? new Promise((resolve) =>
            held.pending.push({
              from,
              settle: () => resolve(real.responseRows(id, from, count)),
            }),
          )
        : real.responseRows(id, from, count),
  };
});

beforeEach(() => {
  held.on = false;
  held.pending = [];
  setUpRequestTabs();
});

const viewer = () => screen.getByRole("document", { name: "Response body" });

async function scrollViewerTo(share: number, max: number): Promise<void> {
  viewer().scrollTop = share * max;
  await fireEvent.scroll(viewer());
}

// jsdom has no layout: the viewer falls back to 400 px.
const VIEWPORT = 400;

test("large_body_fetches_only_visible_rows", async () => {
  const ipc = fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 2_000_000 }) }),
  });
  render(LargeBody, { id: "t1", size: 120 * 1024 * 1024 });
  await screen.findByText("row 0");
  await scrollViewerTo(0.5, 8_000_000 - VIEWPORT);
  await waitFor(() =>
    expect(ipc.rowCalls.at(-1)!.from).toBeGreaterThan(900_000),
  );
  for (const call of ipc.rowCalls) expect(call.count).toBeLessThanOrEqual(200);
});

test("huge_row_counts_scale_the_scroll_space", async () => {
  fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 2_000_000 }) }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0");
  const space = viewer().firstElementChild as HTMLElement;
  expect(space.style.height).toBe("8000000px");
  await scrollViewerTo(1, 8_000_000 - VIEWPORT);
  // The last row is reached at the end of the scroll.
  expect(await screen.findByText("row 1999999")).toBeInTheDocument();
});

test("small_bodies_use_the_row_height", async () => {
  fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 1000 }) }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0");
  expect((viewer().firstElementChild as HTMLElement).style.height).toBe(
    "18000px",
  );
});

test("pages_are_kept_to_twenty", async () => {
  const ipc = fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 100_000 }) }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0");
  for (let page = 1; page <= 25; page++) {
    await scrollViewerTo(
      (page * 200 * 18) / (100_000 * 18 - VIEWPORT),
      100_000 * 18 - VIEWPORT,
    );
  }
  await waitFor(() => expect(ipc.rowCalls.at(-1)!.from).toBeGreaterThan(4000));
  const before = ipc.rowCalls.length;
  await scrollViewerTo(0, 100_000 * 18 - VIEWPORT);
  // Page 0 was dropped, so it is asked for again.
  await waitFor(() => expect(ipc.rowCalls.length).toBeGreaterThan(before));
  expect(ipc.rowCalls.at(-1)!.from).toBe(0);
});

test("find_jumps_to_the_match_row", async () => {
  fakeRequestsIpc({
    response: fakeResponse({
      rows: fakeRows({ total: 50_000, row: (n) => `line ${n} needle here` }),
      find: { row: 40_000, start: 11, end: 17 },
    }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("line 0 needle here");
  viewer().focus();
  await userEvent.keyboard("{Control>}f{/Control}");
  await userEvent.keyboard("needle{Enter}");
  const mark = await screen.findByText("needle", { selector: "mark" });
  expect(mark.parentElement).toHaveTextContent("line 40000 needle here");
  expect(viewer().scrollTop).toBeGreaterThan(40_000 * 18 - VIEWPORT);
});

test("find_reports_no_matches", async () => {
  fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 10 }), find: null }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0");
  viewer().focus();
  await userEvent.keyboard("{Control>}f{/Control}");
  await userEvent.keyboard("zzz{Enter}");
  expect(await screen.findByRole("status")).toHaveTextContent("No matches");
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByLabelText("Find in response")).toBeNull();
});

test("the_card_says_why_and_saves", async () => {
  const ipc = fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 3 }) }),
  });
  render(LargeBody, {
    id: "t1",
    size: 120 * 1024 * 1024,
    name: "List shipments",
    kind: "json",
    contentType: "application/json",
  });
  expect(screen.getByText("The response is 120.0 MB")).toBeInTheDocument();
  expect(
    screen.getByText(
      "Bodies over 10 MB are not kept in memory, so they are shown without formatting.",
    ),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save to file" }));
  await waitFor(() =>
    expect(ipc.savedResponses).toEqual([
      { id: "t1", name: "list-shipments.json" },
    ]),
  );
});

test("the_match_highlight_is_right_after_an_emoji", async () => {
  fakeRequestsIpc({
    response: fakeResponse({
      rows: fakeRows({ total: 10, row: (n) => `row ${n} 😀 needle here` }),
      // Counted in characters: the emoji is one.
      find: { row: 3, start: 8, end: 14 },
    }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0 😀 needle here");
  viewer().focus();
  await userEvent.keyboard("{Control>}f{/Control}");
  await userEvent.keyboard("needle{Enter}");
  expect(await screen.findByText("needle", { selector: "mark" })).toBeVisible();
});

test("late_pages_do_not_push_out_the_rows_in_view", async () => {
  fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 100_000 }) }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0");
  held.on = true;
  // A fast drag passes over many pages, then rests on row 12000.
  for (let row = 1000; row <= 9000; row += 500) {
    viewer().scrollTop = row * 18;
    await fireEvent.scroll(viewer());
  }
  viewer().scrollTop = 12_000 * 18;
  await fireEvent.scroll(viewer());
  const settled = held.pending.length;
  expect(settled).toBeGreaterThan(20);
  // The page in view comes first, the ones passed over after it.
  const [last] = held.pending.splice(-1);
  last!.settle();
  await screen.findByText("row 12000");
  for (const page of held.pending) page.settle();
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(screen.getByText("row 12000")).toBeInTheDocument();
});

test("the_last_rows_are_reachable_in_a_scaled_view", async () => {
  fakeRequestsIpc({
    response: fakeResponse({ rows: fakeRows({ total: 2_000_000 }) }),
  });
  render(LargeBody, { id: "t1", size: 1 });
  await screen.findByText("row 0");
  await scrollViewerTo(1, 8_000_000 - VIEWPORT);
  expect(await screen.findByText("row 1999999")).toBeInTheDocument();
  const window = document.querySelector<HTMLElement>(".window")!;
  const rows = window.querySelectorAll(".row").length;
  // jsdom keeps six significant digits of a length, hence the margin.
  expect(parseFloat(window.style.top) + rows * 18).toBeLessThanOrEqual(
    8_000_000 + 10,
  );
});
