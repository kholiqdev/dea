import { render, screen, within } from "@testing-library/svelte";
import { expect, test } from "vitest";
import type { SseRow } from "../../state/exchange.svelte";
import EventList from "./EventList.svelte";

const row = (n: number, over: Partial<SseRow> = {}): SseRow => ({
  atMs: n * 10,
  event: null,
  data: `data ${n}`,
  id: null,
  ...over,
});

function pad(n: number, width = 2): string {
  return String(n).padStart(width, "0");
}
function clock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

test("events_show_arrival_time_name_and_data", () => {
  const startedAt = 1234.5;
  render(EventList, {
    props: {
      events: [row(1, { event: "tick", data: '{"n":1}', atMs: 250 }), row(2)],
      startedAt,
    },
  });
  const list = screen.getByRole("list", { name: "Server-sent events" });
  const items = within(list).getAllByRole("listitem");
  expect(items).toHaveLength(2);
  const at = clock(performance.timeOrigin + startedAt + 250);
  expect(items[0]).toHaveTextContent(at);
  expect(items[0]).toHaveTextContent("tick");
  expect(items[0]).toHaveTextContent('{"n":1}');
  expect(items[1]).toHaveTextContent("message");
  expect(screen.getByText("2 events")).toBeInTheDocument();
});

test("cancelling_a_stream_keeps_its_events", () => {
  render(EventList, {
    props: { events: [row(1), row(2), row(3)], startedAt: 0 },
  });
  expect(screen.getAllByRole("listitem")).toHaveLength(3);
});

test("only_the_last_ten_thousand_events_are_shown", () => {
  const events = Array.from({ length: 10_005 }, (_, n) => row(n));
  render(EventList, { props: { events, startedAt: 0 } });
  expect(screen.getAllByRole("listitem")).toHaveLength(10_000);
  expect(
    screen.getByText("Showing the last 10,000 events."),
  ).toBeInTheDocument();
  expect(screen.queryByText("data 0")).toBeNull();
  expect(screen.getByText("data 10004")).toBeInTheDocument();
});
