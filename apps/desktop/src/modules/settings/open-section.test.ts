// Opening Settings on one section, as the command palette does.

import { beforeEach, expect, test } from "vitest";
import { manifest } from "./index";
import { sections } from "./state/sections.svelte";

beforeEach(() => {
  sections.current = null;
});

async function openSection(args: unknown): Promise<void> {
  const intent = manifest.intents?.find(
    (i) => i.intent === "settings.openSection",
  );
  const { default: run } = await intent!.handler();
  await run(args);
}

test("Settings opens on the section chosen in the palette", async () => {
  await openSection({ section: "settings.security" });
  expect(sections.current).toBe("settings.security");
});

test("a request without a section changes nothing", async () => {
  sections.current = "settings.shortcuts";
  await openSection({});
  await openSection(undefined);
  expect(sections.current).toBe("settings.shortcuts");
});
