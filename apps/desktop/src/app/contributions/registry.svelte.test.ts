import { expect, test } from "vitest";
import { ContributionError, createRegistry } from "./registry.svelte";
import type { ModuleManifest } from "./types";

const view = () => Promise.resolve({ default: (() => {}) as never });

function manifest(
  id: string,
  extra: Partial<ModuleManifest> = {},
): ModuleManifest {
  return { id, title: id, ...extra };
}

function cmd(id: string) {
  return {
    id,
    title: id,
    action: { type: "openModule" as const, module: "x" },
  };
}

test("rejects a duplicate id and names it", () => {
  const r = createRegistry();
  r.register(manifest("requests", { commands: [cmd("requests.send")] }));
  expect(() =>
    r.register(manifest("other", { commands: [cmd("requests.send")] })),
  ).toThrow('Duplicate contribution id "requests.send"');
});

test("rejects a duplicate id inside one manifest", () => {
  const r = createRegistry();
  expect(() =>
    r.register(
      manifest("requests", {
        commands: [cmd("requests.send")],
        tabTypes: [{ id: "requests.send", title: "x", icon: "x", view }],
      }),
    ),
  ).toThrow('Duplicate contribution id "requests.send"');
});

test("rejects ids outside the module namespace", () => {
  const r = createRegistry();
  expect(() =>
    r.register(manifest("requests", { commands: [cmd("send")] })),
  ).toThrow(ContributionError);
  expect(() =>
    r.register(manifest("requests", { commands: [cmd("database.x")] })),
  ).toThrow(ContributionError);
  expect(() =>
    r.register(manifest("requests", { commands: [cmd("requests.Send")] })),
  ).toThrow(ContributionError);
  expect(r.commands).toEqual([]);
});

test("a failed registration leaves nothing behind", () => {
  const r = createRegistry();
  expect(() =>
    r.register(
      manifest("requests", {
        rail: { icon: "send", group: "main", order: 1 },
        commands: [cmd("requests.open"), cmd("bad")],
      }),
    ),
  ).toThrow(ContributionError);
  expect(r.modules).toEqual([]);
  r.register(manifest("requests", { commands: [cmd("requests.open")] }));
  expect(r.commands.map((c) => c.id)).toEqual(["requests.open"]);
});

test("rejects a module registered twice", () => {
  const r = createRegistry();
  r.register(manifest("requests"));
  expect(() => r.register(manifest("requests"))).toThrow(
    'Duplicate module id "requests"',
  );
});

test("rail items are ordered by group then order", () => {
  const r = createRegistry();
  r.register(
    manifest("settings", { rail: { icon: "s", group: "bottom", order: 2 } }),
  );
  r.register(
    manifest("terminal", { rail: { icon: "t", group: "main", order: 3 } }),
  );
  r.register(
    manifest("vault", { rail: { icon: "v", group: "bottom", order: 1 } }),
  );
  r.register(manifest("project"));
  r.register(
    manifest("requests", { rail: { icon: "r", group: "main", order: 1 } }),
  );
  expect(r.modules.map((m) => m.id)).toEqual([
    "requests",
    "terminal",
    "vault",
    "settings",
  ]);
});

test("disabling a module removes every contribution", async () => {
  const r = createRegistry();
  const handler = () => Promise.resolve({ default: (args: unknown) => args });
  r.register(
    manifest("requests", {
      rail: { icon: "r", group: "main", order: 1 },
      tabTypes: [{ id: "requests.request", title: "Request", icon: "r", view }],
      commands: [cmd("requests.open")],
      shortcuts: [{ command: "requests.open", chord: "Mod+1" }],
      statusItems: [{ id: "requests.status", align: "end", order: 1, view }],
      settings: [
        { id: "requests.settings", title: "Requests", order: 5, view },
      ],
      intents: [{ intent: "openInRequests", handler }],
    }),
  );
  r.register(
    manifest("database", {
      rail: { icon: "d", group: "main", order: 2 },
      commands: [cmd("database.open")],
      shortcuts: [{ command: "database.open", chord: "Mod+2" }],
    }),
  );

  r.setEnabled("requests", false);

  expect(r.modules.map((m) => m.id)).toEqual(["database"]);
  expect(r.commands.map((c) => c.id)).toEqual(["database.open"]);
  expect(r.shortcuts.map((s) => s.command)).toEqual(["database.open"]);
  expect(r.statusItems).toEqual([]);
  expect(r.settingsSections).toEqual([]);
  expect(r.tabType("requests.request")).toBeUndefined();
  expect(await r.invoke("openInRequests", 1)).toEqual({
    ok: false,
    message: 'No module handles "openInRequests"',
  });

  r.setEnabled("requests", true);
  expect(r.tabType("requests.request")?.title).toBe("Request");
  expect(await r.invoke("openInRequests", 1)).toEqual({ ok: true, value: 1 });
});

test("an intent without a handler returns a message naming it", async () => {
  expect(await createRegistry().invoke("openInRequests")).toEqual({
    ok: false,
    message: 'No module handles "openInRequests"',
  });
});

test("a handler that throws is reported, not raised", async () => {
  const r = createRegistry();
  r.register(
    manifest("requests", {
      intents: [
        {
          intent: "openInRequests",
          handler: () =>
            Promise.resolve({
              default: () => {
                throw new Error("bad request");
              },
            }),
        },
      ],
    }),
  );
  expect(await r.invoke("openInRequests")).toEqual({
    ok: false,
    message: "bad request",
  });
});

test("sections and status items are ordered", () => {
  const r = createRegistry();
  r.register(
    manifest("settings", {
      settings: [
        { id: "settings.shortcuts", title: "Shortcuts", order: 2, view },
        { id: "settings.appearance", title: "Appearance", order: 1, view },
      ],
      statusItems: [
        { id: "settings.b", align: "end", order: 2, view },
        { id: "settings.a", align: "end", order: 1, view },
      ],
    }),
  );
  expect(r.settingsSections.map((s) => s.id)).toEqual([
    "settings.appearance",
    "settings.shortcuts",
  ]);
  expect(r.statusItems.map((s) => s.id)).toEqual(["settings.a", "settings.b"]);
});

test("without a project only app-wide modules remain", () => {
  const r = createRegistry();
  r.register(manifest("requests", { commands: [cmd("requests.open")] }));
  r.register(
    manifest("settings", { scope: "app", commands: [cmd("settings.open")] }),
  );
  r.setProjectOpen(false);
  expect(r.commands.map((c) => c.id)).toEqual(["settings.open"]);
  expect(r.module("requests")).toBeUndefined();
  r.setProjectOpen(true);
  expect(r.commands.map((c) => c.id)).toEqual([
    "requests.open",
    "settings.open",
  ]);
});

const source = (id: string, order = 1) => ({
  id,
  group: id,
  order,
  source: () => Promise.resolve({ default: () => [] }),
});

test("search sources come only from enabled modules", () => {
  const r = createRegistry();
  r.register(
    manifest("vault", { searchSources: [source("vault.secrets", 70)] }),
  );
  r.register(
    manifest("project", {
      searchSources: [source("project.environments", 60)],
    }),
  );
  expect(r.searchSources.map((s) => [s.id, s.module])).toEqual([
    ["project.environments", "project"],
    ["vault.secrets", "vault"],
  ]);
  r.setEnabled("vault", false);
  expect(r.searchSources.map((s) => s.id)).toEqual(["project.environments"]);
});

test("project search sources wait for a project", () => {
  const r = createRegistry();
  r.register(manifest("vault", { searchSources: [source("vault.secrets")] }));
  r.register(
    manifest("settings", {
      scope: "app",
      searchSources: [source("settings.sections")],
    }),
  );
  r.setProjectOpen(false);
  expect(r.searchSources.map((s) => s.id)).toEqual(["settings.sections"]);
});

test("a duplicate search source id is refused, naming it", () => {
  const r = createRegistry();
  r.register(manifest("vault", { searchSources: [source("vault.secrets")] }));
  expect(() =>
    r.register(manifest("other", { searchSources: [source("vault.secrets")] })),
  ).toThrow(ContributionError);
  expect(() =>
    r.register(manifest("third", { searchSources: [source("vault.secrets")] })),
  ).toThrow('"vault.secrets"');
});
