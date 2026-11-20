// The Requests sidebar and editor (F1-REQ-01, F1-REQ-02): the tree's file
// operations and their order on disk, dragging, every part of the editor,
// the JSON body, a file outside the project, and the filter. Each case
// opens its own small project, so none depends on another.

import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { Key } from "webdriverio";
import {
  MOD,
  dragTo,
  openProjectAt,
  pickNextFile,
  textOf,
  waitForNoDialog,
  waitForText,
  writeProject,
} from "./helpers.js";

const work = mkdtempSync(join(tmpdir(), "dea-e2e-requests-"));
let count = 0;

/** A project with the collection Shipments: three requests and a folder. */
function shipmentsProject(): string {
  const root = writeProject(`shipments-${++count}`, work);
  const files: Record<string, string> = {
    "requests/shipments/_folder.yaml":
      "name: Shipments\norder: [list-shipments, create-shipment, get-shipment, tracking]\n",
    "requests/shipments/list-shipments.yaml":
      "name: List shipments\nmethod: GET\nurl: https://api.example.com/shipments\n",
    "requests/shipments/create-shipment.yaml":
      "name: Create shipment\nmethod: POST\nurl: https://api.example.com/v2/parcels\n",
    "requests/shipments/get-shipment.yaml":
      "name: Get shipment\nmethod: GET\nurl: https://api.example.com/shipments/42\n",
    "requests/shipments/tracking/_folder.yaml": "name: Tracking\n",
  };
  for (const [file, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    writeFileSync(join(root, file), text);
  }
  return root;
}

const SHIPMENTS = "requests/shipments";

function read(root: string, file: string): string {
  return readFileSync(join(root, file), "utf8");
}

/** The `order` of a folder's _folder.yaml, written as a list or in brackets. */
function orderOf(root: string, folder: string): string[] {
  const text = read(root, `${folder}/_folder.yaml`);
  const inline = /^order:\s*\[(.*)\]\s*$/m.exec(text);
  const items = inline
    ? (inline[1] ?? "").split(",")
    : [...text.matchAll(/^\s*-\s+(.+)$/gm)].map((m) => m[1] ?? "");
  return items.map((i) => i.trim().replace(/^["']|["']$/g, "")).filter(Boolean);
}

async function until(condition: () => boolean, what: string): Promise<void> {
  await browser.waitUntil(condition, {
    timeout: 15_000,
    timeoutMsg: `never happened: ${what}`,
  });
}

/** The tree row named `name`. */
function row(name: string) {
  return $(
    `//*[@role="treeitem"][.//span[contains(@class,"name") and normalize-space(.)="${name}"]]`,
  );
}

/** The names of the rows the tree shows. */
function rowNames(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[role="treeitem"] .name')].map(
      (e) => e.textContent?.trim() ?? "",
    ),
  );
}

/** Opens a folder row that is closed. */
async function expand(name: string): Promise<void> {
  const item = row(name);
  await item.waitForDisplayed();
  if ((await item.getAttribute("aria-expanded")) === "false")
    await item.click();
  await browser.waitUntil(
    async () => (await item.getAttribute("aria-expanded")) === "true",
  );
}

/**
 * Chooses `entry` from the menu of the row named `name`. A right click is
 * the riskiest step under WebKitGTK, so a request row is clicked and its
 * More actions button opens the menu. A click would close a folder, so a
 * folder must be the selected row already, as `expand` leaves it, and
 * Shift+F10 on the tree opens its menu.
 */
async function rowMenu(name: string, entry: string): Promise<void> {
  const item = row(name);
  await item.waitForDisplayed();
  if ((await item.getAttribute("aria-expanded")) === null) {
    await item.click();
    await browser.waitUntil(
      async () => (await item.getAttribute("aria-selected")) === "true",
      { timeoutMsg: `${name} was not selected` },
    );
    await $('.actions button[title="More actions"]').click();
  } else {
    if ((await item.getAttribute("aria-selected")) !== "true")
      throw new Error(`Select the folder ${name} first: a click closes it`);
    await browser.execute(() =>
      document.querySelector<HTMLElement>('[role="tree"]')?.focus(),
    );
    await browser.keys([Key.Shift, Key.F10]);
  }
  const menu = $('[role="menu"]');
  await menu.waitForDisplayed();
  await menu.$(`button*=${entry}`).click();
}

/** Types into the field that names a new item, and saves it with Enter. */
async function nameIt(name: string): Promise<void> {
  await $('input[aria-label="Name"]').waitForDisplayed();
  // The field starts with its text selected. Clearing it from outside would
  // blur it, which saves.
  await browser.keys(name);
  await browser.keys(Key.Enter);
}

async function openShipments(root: string): Promise<void> {
  await openProjectAt(root);
  await $('button[data-module="requests"]').click();
  await expand("Shipments");
}

/** Opens a request in a tab and waits for the editor. */
async function openRequest(name: string): Promise<void> {
  await row(name).click();
  await $('[role="tablist"][aria-label="Request sections"]').waitForDisplayed();
}

async function section(name: string): Promise<void> {
  await $('[role="tablist"][aria-label="Request sections"]')
    .$(`button=${name}`)
    .click();
}

/** Writes in the code editor of the Body section. */
async function typeInBody(text: string, replace = false): Promise<void> {
  const body = $('.cm-content[aria-label="Body"]');
  await body.waitForDisplayed();
  await body.click();
  if (replace) await browser.keys([MOD, "a"]);
  await browser.keys(text);
}

describe("the Requests sidebar and editor", () => {
  after(() => rmSync(work, { recursive: true, force: true }));

  it("creates_renames_duplicates_and_deletes_with_undo", async () => {
    const root = shipmentsProject();
    await openShipments(root);

    // Create: the new request is named in place and goes last.
    await rowMenu("Shipments", "New request");
    await nameIt("Cancel shipment");
    const cancel = `${SHIPMENTS}/cancel-shipment.yaml`;
    await until(() => existsSync(join(root, cancel)), "the renamed file");
    expect(read(root, cancel)).toContain("name: Cancel shipment");
    expect(existsSync(join(root, `${SHIPMENTS}/new-request.yaml`))).toBe(false);
    const created = [
      "list-shipments",
      "create-shipment",
      "get-shipment",
      "tracking",
      "cancel-shipment",
    ];
    await until(
      () => orderOf(root, SHIPMENTS).join() === created.join(),
      "the order after create and rename",
    );

    // Duplicate one in the middle: the copy sits right after the original.
    await rowMenu("Create shipment", "Duplicate");
    const copy = `${SHIPMENTS}/create-shipment-copy.yaml`;
    await until(() => existsSync(join(root, copy)), "the copy");
    expect(read(root, copy)).toContain("name: Create shipment copy");
    const duplicated = [
      "list-shipments",
      "create-shipment",
      "create-shipment-copy",
      "get-shipment",
      "tracking",
      "cancel-shipment",
    ];
    await until(
      () => orderOf(root, SHIPMENTS).join() === duplicated.join(),
      "the order after duplicate",
    );

    // Delete, then Undo from the toast.
    await rowMenu("Create shipment copy", "Delete");
    await waitForText('[role="dialog"]', "Delete Create shipment copy?");
    await $('[role="dialog"]').$("button=Delete").click();
    await waitForNoDialog();
    await until(() => !existsSync(join(root, copy)), "the file to go");
    await until(
      () => orderOf(root, SHIPMENTS).join() === created.join(),
      "the order after delete",
    );
    await waitForText('[role="status"]', "Deleted Create shipment copy");
    await $('[role="status"]').$("button*=Undo").click();
    await until(() => existsSync(join(root, copy)), "Undo to bring it back");
    await until(
      () => orderOf(root, SHIPMENTS).join() === duplicated.join(),
      "the order after Undo",
    );

    // Delete again, and Undo with Mod+Z, as no text field has the focus.
    await rowMenu("Create shipment copy", "Delete");
    await $('[role="dialog"]').$("button=Delete").click();
    await waitForNoDialog();
    await until(() => !existsSync(join(root, copy)), "the file to go again");
    await waitForText('[role="status"]', "Deleted Create shipment copy");
    await browser.keys([MOD, "z"]);
    await until(() => existsSync(join(root, copy)), "Mod+Z to bring it back");
    await until(
      () => orderOf(root, SHIPMENTS).join() === duplicated.join(),
      "the order after Mod+Z",
    );
  });

  it("drags_a_request_into_another_folder", async () => {
    const root = shipmentsProject();
    await openShipments(root);

    // Into the middle of the folder: the request goes last in it.
    await dragTo(row("Get shipment"), row("Tracking"), "into");
    const moved = `${SHIPMENTS}/tracking/get-shipment.yaml`;
    await until(() => existsSync(join(root, moved)), "the moved file");
    expect(existsSync(join(root, `${SHIPMENTS}/get-shipment.yaml`))).toBe(
      false,
    );
    await until(
      () =>
        orderOf(root, SHIPMENTS).join() ===
        "list-shipments,create-shipment,tracking",
      "the order of Shipments",
    );
    expect(orderOf(root, `${SHIPMENTS}/tracking`)).toEqual(["get-shipment"]);

    // Below another row: a new place in the same folder.
    await dragTo(row("List shipments"), row("Create shipment"), "below");
    await until(
      () =>
        orderOf(root, SHIPMENTS).join() ===
        "create-shipment,list-shipments,tracking",
      "the order after the second drag",
    );
    expect(orderOf(root, `${SHIPMENTS}/tracking`)).toEqual(["get-shipment"]);
  });

  it("edits_every_part_and_saves_with_mod_s", async () => {
    const root = shipmentsProject();
    await openShipments(root);
    await openRequest("List shipments");

    // The method, from its menu.
    await $('.method button[aria-haspopup="menu"]').click();
    await $('[role="menu"]').$("button*=PATCH").click();
    await waitForText('.method button[aria-haspopup="menu"]', "PATCH");

    // The URL, with a path variable and a query.
    const url = $('input[aria-label="URL"]');
    await url.click();
    await browser.keys([MOD, "a"]);
    await browser.keys(
      "https://api.example.com/shipments/:id?limit=20&sort=asc",
    );
    await section("Params");
    await $('input[aria-label="Value of id"]').click();
    await browser.keys("99");
    // A query row can be switched off and stays in the file.
    await $('input[aria-label="Send sort"]').click();

    // A header.
    await section("Headers");
    await $("button=Add header").click();
    await browser.keys("X-Trace");
    await $('input[aria-label="Value of X-Trace"]').click();
    await browser.keys("abc123");

    // A JSON body.
    await section("Body");
    await $('[role="radiogroup"]').$("label=JSON").click();
    await typeInBody('{"parcel": "p-42"}');

    await browser.keys([MOD, "s"]);
    const file = `${SHIPMENTS}/list-shipments.yaml`;
    await until(
      () => read(root, file).includes("method: PATCH"),
      "Mod+S to write the file",
    );
    const text = read(root, file);
    expect(text).toContain("url: https://api.example.com/shipments/:id");
    expect(text).toMatch(/name: id\s+value: ['"]?99['"]?/);
    expect(text).toMatch(/name: limit\s+value: ['"]?20['"]?/);
    expect(text).toMatch(/name: sort\s+value: asc\s+enabled: false/);
    expect(text).toMatch(/name: X-Trace\s+value: abc123/);
    expect(text).toContain("mode: json");
    expect(text).toContain("p-42");
  });

  it("json_error_names_the_line_and_format_fixes_spacing", async () => {
    const root = shipmentsProject();
    await openShipments(root);
    await openRequest("Create shipment");
    await section("Body");
    await $('[role="radiogroup"]').$("label=JSON").click();

    // A trailing comma: the error is on the line of the closing brace.
    await typeInBody('{"a": 1,');
    await browser.keys(Key.Enter);
    await browser.keys("}");
    await waitForText(".code-editor .status", "Line 2: ");
    await expect($("button.format")).toBeDisabled();

    // Valid but cramped text: Format spaces it out.
    await typeInBody('{"a":1,"b":[true,null]}', true);
    await browser.waitUntil(
      async () => (await textOf(".code-editor .status")) === "",
      { timeoutMsg: "the error did not go away" },
    );
    await browser.waitUntil(async () => await $("button.format").isEnabled());
    await $("button.format").click();
    await browser.waitUntil(
      async () =>
        (
          await browser.execute(
            () =>
              document.querySelector('.cm-content[aria-label="Body"]')
                ?.textContent ?? "",
          )
        ).includes('"a": 1'),
      { timeoutMsg: "Format did not space the JSON" },
    );

    await browser.keys([MOD, "s"]);
    await until(
      () => read(root, `${SHIPMENTS}/create-shipment.yaml`).includes('"a": 1'),
      "the formatted body on disk",
    );
  });

  it("outside_file_warns_and_is_saved_as_a_local_ref", async () => {
    const root = shipmentsProject();
    const outside = join(work, `outside-${count}`);
    mkdirSync(outside);
    const label = join(outside, "parcel-label.txt");
    writeFileSync(label, "label");
    await openShipments(root);
    await openRequest("Create shipment");
    await section("Body");
    await $('[role="radiogroup"]').$("label=Binary").click();

    pickNextFile(label);
    await $("button*=Choose").click();
    await waitForText(
      ".choice .note",
      "parcel-label.txt is outside the project, so it is not committed.",
    );

    await browser.keys([MOD, "s"]);
    const file = `${SHIPMENTS}/create-shipment.yaml`;
    await until(
      () => /file: local:[0-9a-f]{12}\b/.test(read(root, file)),
      "a local reference in the file",
    );
    // The machine's path is not in the file that is committed.
    expect(read(root, file)).not.toContain(outside);
    expect(read(root, file)).not.toContain("parcel-label");
  });

  it("filter_finds_by_url", async () => {
    const root = shipmentsProject();
    await openShipments(root);

    const filter = $("#requests-filter");
    await filter.click();
    await browser.keys("parcels");
    await browser.waitUntil(
      async () => (await rowNames()).join() === "Shipments,Create shipment",
      { timeoutMsg: "the filter did not find the request by its URL" },
    );

    // Back to everything, then a name.
    await browser.keys([MOD, "a"]);
    await browser.keys(Key.Backspace);
    await browser.keys("list");
    await browser.waitUntil(
      async () => (await rowNames()).join() === "Shipments,List shipments",
      { timeoutMsg: "the filter did not find the request by its name" },
    );
    expect((await $$('[role="treeitem"]')).length).toBe(2);
  });
});
