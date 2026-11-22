import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test } from "vitest";
import type { TimingInfo } from "$lib/ipc/bindings/TimingInfo";
import { sessionView } from "../../state/session-view.svelte";
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

afterEach(() => {
  sessionView.timingLegend = false;
});

/** The share of the bar a segment takes, from its inline style. */
function segmentWidth(bar: HTMLElement, name: string): number {
  const segment = bar.querySelector<HTMLElement>(`[data-segment="${name}"]`);
  if (!segment) throw new Error(`No segment ${name}`);
  return parseFloat(segment.style.width) / 100;
}

test("timing_bar_widths_follow_the_phases", () => {
  render(TimingBar, { timing, controls: "timing-legend" });
  const bar = screen.getByRole("button", {
    name: "Timing, 148 ms total, DNS 4, Connect 12, TLS 18, Waiting for server 96, Download 18",
  });
  expect(segmentWidth(bar, "waiting")).toBeCloseTo(96 / 148, 2);
  expect(segmentWidth(bar, "dns")).toBeCloseTo(4 / 148, 2);
  expect(segmentWidth(bar, "download")).toBeCloseTo(18 / 148, 2);
});

test("reused_connections_say_so_and_take_no_width", () => {
  render(TimingBar, {
    timing: {
      ...timing,
      dnsMs: 0,
      connectMs: 0,
      tlsMs: 0,
      totalMs: 114,
      reused: true,
    },
    controls: "timing-legend",
  });
  const bar = screen.getByRole("button", {
    name: "Timing, 114 ms total, DNS 0, Connect 0, TLS 0, Waiting for server 96, Download 18, connection reused",
  });
  expect(segmentWidth(bar, "dns")).toBe(0);
});

test("the_bar_opens_and_closes_the_legend_for_the_session", async () => {
  render(TimingBar, { timing, controls: "timing-legend" });
  const bar = screen.getByRole("button", { name: /^Timing,/ });
  expect(bar).toHaveAttribute("aria-controls", "timing-legend");
  expect(bar).toHaveAttribute("aria-expanded", "false");
  await userEvent.click(bar);
  expect(sessionView.timingLegend).toBe(true);
  expect(bar).toHaveAttribute("aria-expanded", "true");
  await userEvent.click(bar);
  expect(sessionView.timingLegend).toBe(false);
  expect(bar).toHaveAttribute("aria-expanded", "false");
});

test("a_muted_bar_is_neutral", () => {
  render(TimingBar, { timing, controls: "timing-legend", muted: true });
  const segments = document.querySelectorAll<HTMLElement>("[data-segment]");
  expect(segments).toHaveLength(5);
  for (const segment of segments)
    expect(segment.getAttribute("style")).toContain(
      "background: var(--text-3)",
    );
});
