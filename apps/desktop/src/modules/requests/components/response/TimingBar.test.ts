import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import type { TimingInfo } from "$lib/ipc/bindings/TimingInfo";
import TimingBar from "./TimingBar.svelte";

const timing: TimingInfo = {
  dnsMs: 4,
  connectMs: 12,
  tlsMs: 18,
  waitingMs: 96,
  downloadMs: 18,
  totalMs: 148,
  reused: false,
};

/** The share of the bar a segment takes, from its inline style. */
function segmentWidth(bar: HTMLElement, name: string): number {
  const segment = bar.querySelector<HTMLElement>(`[data-segment="${name}"]`);
  if (!segment) throw new Error(`No segment ${name}`);
  return parseFloat(segment.style.width) / 100;
}

test("timing_bar_widths_follow_the_phases", () => {
  render(TimingBar, { timing });
  const bar = screen.getByRole("img", {
    name: "Timing, 148 ms total, DNS 4, Connect 12, TLS 18, Waiting for server 96, Download 18",
  });
  expect(segmentWidth(bar, "waiting")).toBeCloseTo(96 / 148, 2);
  expect(segmentWidth(bar, "dns")).toBeCloseTo(4 / 148, 2);
  expect(segmentWidth(bar, "download")).toBeCloseTo(18 / 148, 2);
  expect(screen.getByText("96 ms")).toBeInTheDocument();
});

test("reused_connections_show_zero_and_reused", () => {
  render(TimingBar, {
    timing: {
      ...timing,
      dnsMs: 0,
      connectMs: 0,
      tlsMs: 0,
      totalMs: 114,
      reused: true,
    },
  });
  const bar = screen.getByRole("img", {
    name: "Timing, 114 ms total, DNS 0, Connect 0, TLS 0, Waiting for server 96, Download 18, connection reused",
  });
  expect(segmentWidth(bar, "dns")).toBe(0);
  expect(screen.getAllByText("0 ms")).toHaveLength(3);
  expect(screen.getAllByText("reused")).toHaveLength(1);
});
