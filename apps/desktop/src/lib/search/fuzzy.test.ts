import { expect, test } from "vitest";
import { rank, score } from "./fuzzy";

const same = (s: string) => s;

test("ranking_matches_sample_queries", () => {
  const items = [
    "Shipment by ID",
    "Create shipment",
    "Lock Vault",
    "shipments",
    "Show settings",
    "access_token",
    "Choose the environment for requests",
  ];
  expect(rank("shp", items, same, 3)).toEqual([
    "shipments",
    "Shipment by ID",
    "Create shipment",
  ]);
  expect(rank("lv", items, same, 1)).toEqual(["Lock Vault"]);
  expect(rank("acc tok", items, same, 1)).toEqual(["access_token"]);
  expect(rank("env", items, same, 1)).toEqual([
    "Choose the environment for requests",
  ]);
  expect(score("xyz", "Lock Vault")).toBeNull();
});

test("an empty query keeps the first items in their order", () => {
  expect(rank("  ", ["b", "a", "c"], same, 2)).toEqual(["b", "a"]);
});

test("odd_queries_never_throw", () => {
  for (const q of [".*(", "  shp  ", "x".repeat(500), "é", "Ж"])
    expect(() => rank(q, ["Café", "Жук", "a.*(b"], same, 5)).not.toThrow();
  expect(rank("cafe", ["Café"], same, 1)).toEqual(["Café"]);
  expect(rank(".*(", ["a.*(b", "abc"], same, 5)).toEqual(["a.*(b"]);
  expect(rank("  shp  ", ["shipments"], same, 1)).toEqual(["shipments"]);
});

test("ranking 5,000 items takes under 100 ms", () => {
  const words = ["shipment", "carrier", "tracking", "invoice", "parcel"];
  const items = Array.from(
    { length: 5000 },
    (_, i) => `${words[i % 5]} ${i} ${words[(i * 7) % 5]} by id`,
  );
  const runs: number[] = [];
  for (let r = 0; r < 5; r++) {
    const start = performance.now();
    rank("ship", items, same, 50);
    runs.push(performance.now() - start);
  }
  runs.sort((a, b) => a - b);
  expect(runs[2]).toBeLessThan(100);
});
