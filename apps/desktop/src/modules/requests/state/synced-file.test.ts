import { expect, test } from "vitest";
import { SyncedFile } from "./synced-file.svelte";

/** A text file whose reads and prepare step can be held. */
class Notes extends SyncedFile<{ text: string }> {
  draft = { text: "" };
  disk = { text: "base" };
  writes: string[] = [];
  holdReads: (() => void)[] | null = null;
  holdPrepare: (() => void)[] | null = null;
  failReads = false;

  get noun() {
    return "note";
  }
  protected readFile(): Promise<{ text: string }> {
    if (this.failReads) return Promise.reject(new Error("boom"));
    const held = this.holdReads;
    if (!held) return Promise.resolve({ ...this.disk });
    // The disk is looked at when the read is released, not when it starts.
    return new Promise((resolve) => held.push(() => resolve({ ...this.disk })));
  }
  protected writeFile(_path: string, file: { text: string }): Promise<void> {
    this.writes.push(file.text);
    this.disk = { ...file };
    return Promise.resolve();
  }
  protected snapshot() {
    return { ...this.draft };
  }
  protected clean(file: { text: string }) {
    return file;
  }
  protected same(a: { text: string }, b: { text: string }) {
    return a.text === b.text;
  }
  protected override prepare(file: { text: string }) {
    const held = this.holdPrepare;
    if (!held) return Promise.resolve(file);
    return new Promise<{ text: string }>((resolve) =>
      held.push(() => resolve(file)),
    );
  }
}

async function open(): Promise<Notes> {
  const notes = new Notes("a.txt");
  await notes.load();
  notes.draft.text = "mine";
  return notes;
}

test("a_save_waits_for_a_disk_check_in_flight", async () => {
  const notes = await open();
  notes.holdReads = [];
  const reads = notes.holdReads;
  void notes.check();
  const saving = notes.save();
  notes.disk = { text: "theirs" };
  notes.holdReads = null;
  reads.forEach((release) => release());
  await expect(saving).rejects.toThrow(/changed on disk/);
  expect(notes.outside).toBe("changed");
  expect(notes.writes).toHaveLength(0);
});

test("a_check_that_lands_during_prepare_stops_the_write", async () => {
  const notes = await open();
  notes.holdPrepare = [];
  const prepared = notes.holdPrepare;
  const saving = notes.save();
  await Promise.resolve();
  await Promise.resolve();
  expect(prepared).toHaveLength(1);
  notes.disk = { text: "theirs" };
  await notes.check();
  expect(notes.outside).toBe("changed");
  prepared.forEach((release) => release());
  await expect(saving).rejects.toThrow(/changed on disk/);
  expect(notes.writes).toHaveLength(0);
});

test("a_failed_check_does_not_block_later_saves", async () => {
  const notes = await open();
  notes.failReads = true;
  await notes.check();
  notes.failReads = false;
  await notes.save();
  expect(notes.writes).toEqual(["mine"]);
});
