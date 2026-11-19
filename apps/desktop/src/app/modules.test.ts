import { expect, test } from "vitest";
import { createRegistry } from "./contributions/registry.svelte";
import { BUILT_IN_MODULES, railIcon, registerBuiltInModules } from "./modules";

const IDS = [
  "requests",
  "database",
  "terminal",
  "mobile",
  "vault",
  "settings",
  "project",
];

test("every built-in module registers without errors", () => {
  const r = createRegistry();
  registerBuiltInModules(r);
  expect(BUILT_IN_MODULES.map((m) => m.id)).toEqual(IDS);
  expect(r.modules.map((m) => m.id)).toEqual([
    "requests",
    "database",
    "terminal",
    "mobile",
    "vault",
    "settings",
  ]);
  for (const m of r.modules) expect(railIcon(m.rail!.icon), m.id).toBeDefined();
  for (const id of IDS) expect(r.module(id)?.view, id).toBeTypeOf("function");
});

test.each(IDS)("disabling %s leaves the others working", (id) => {
  const r = createRegistry();
  registerBuiltInModules(r);
  r.setEnabled(id, false);
  expect(r.modules.map((m) => m.id)).not.toContain(id);
  expect(r.commands.filter((c) => c.module === id)).toEqual([]);
  expect(r.shortcuts.filter((s) => s.module === id)).toEqual([]);
  for (const other of IDS.filter((o) => o !== id)) {
    const m = r.module(other);
    expect(m, other).toBeDefined();
    if (m?.rail) expect(r.modules).toContain(m);
    expect(
      r.commands.some((c) => c.module === other),
      other,
    ).toBe(true);
  }
});

test("Mod+1 to Mod+4 open the four main modules", () => {
  const r = createRegistry();
  registerBuiltInModules(r);
  const opens = (chord: string) => {
    const shortcut = r.shortcuts.find((s) => s.chord === chord);
    const command = r.commands.find((c) => c.id === shortcut?.command);
    return command?.action;
  };
  expect(opens("Mod+1")).toEqual({ type: "openModule", module: "requests" });
  expect(opens("Mod+2")).toEqual({ type: "openModule", module: "database" });
  expect(opens("Mod+3")).toEqual({ type: "openModule", module: "terminal" });
  expect(opens("Mod+4")).toEqual({ type: "openModule", module: "mobile" });
  expect(opens("Mod+,")).toEqual({ type: "openModule", module: "settings" });
});

test("every view loads", async () => {
  for (const m of BUILT_IN_MODULES) {
    const loaded = await m.view!();
    expect(loaded.default, m.id).toBeTypeOf("function");
  }
});
