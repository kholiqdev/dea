import { expect, test } from "vitest";
import { expiresIn, relativeTime } from "./relative";

const NOW = new Date("2026-11-19T12:00:00Z");
const ago = (ms: number) => new Date(NOW.getTime() - ms).toISOString();
const ahead = (ms: number) => new Date(NOW.getTime() + ms).toISOString();
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

test("relative times and expiry read like the design", () => {
  expect(relativeTime(ago(20_000), NOW)).toBe("just now");
  expect(relativeTime(ago(8 * MIN), NOW)).toBe("8 min ago");
  expect(relativeTime(ago(HOUR), NOW)).toBe("1 hour ago");
  expect(relativeTime(ago(3 * HOUR), NOW)).toBe("3 hours ago");
  expect(relativeTime(ago(DAY), NOW)).toBe("1 day ago");
  expect(relativeTime(ago(12 * DAY), NOW)).toBe("12 days ago");
  expect(relativeTime(ago(65 * DAY), NOW)).toBe("2 months ago");
  expect(relativeTime(ahead(MIN), NOW)).toBe("just now");

  expect(expiresIn(ahead(40 * MIN), NOW)).toBe("Expires in 40 min");
  expect(expiresIn(ahead(3 * HOUR), NOW)).toBe("Expires in 3 h");
  expect(expiresIn(ahead(2 * DAY), NOW)).toBe("Expires in 2 days");
  expect(expiresIn(ahead(DAY), NOW)).toBe("Expires in 1 day");
  expect(expiresIn(ago(MIN), NOW)).toBe("Expired");
});
