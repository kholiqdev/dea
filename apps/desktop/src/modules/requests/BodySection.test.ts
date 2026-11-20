import { screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { Body } from "$lib/ipc/bindings/Body";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import { openRequestTab, setUpRequestTabs } from "./request-tab-support.svelte";
import { resetTreeState } from "./state/tree.svelte";
import {
  fakeRequestsIpc,
  folder,
  leaf,
  type RequestsIpc,
} from "./test-support";

const UPLOAD = "requests/shipments/upload.yaml";

beforeEach(() => {
  resetTreeState();
  setUpRequestTabs();
});

function uploadIpc(
  body?: Body,
  extra: Parameters<typeof fakeRequestsIpc>[0] = {},
): RequestsIpc {
  const file: RequestFile = {
    name: "Upload",
    method: "POST",
    url: "https://harbor.test/upload",
    ...(body ? { body } : {}),
  };
  return fakeRequestsIpc({
    tree: {
      collections: [
        folder("requests/shipments", "Shipments", [
          leaf(UPLOAD, "Upload", "POST", file.url),
        ]),
      ],
      errors: [],
    },
    files: { [UPLOAD]: file },
    ...extra,
  });
}

async function openBody(): Promise<void> {
  openRequestTab(UPLOAD);
  await userEvent.click(await screen.findByRole("tab", { name: /Body/ }));
}

/** CodeMirror keeps a handle to its view on the content node. */
interface CmView {
  state: { doc: { length: number; toString(): string } };
  dispatch(spec: {
    changes: { from: number; to: number; insert: string };
  }): void;
}
interface CmNode {
  cmTile: { root: { view: CmView } };
}
async function editor(): Promise<CmView> {
  const content = await screen.findByRole("textbox", { name: "Body" });
  return (content as unknown as CmNode).cmTile.root.view;
}

async function type(text: string): Promise<void> {
  const view = await editor();
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
  });
}

const mode = (name: string) => screen.findByRole("radio", { name });

test("an outside file is kept as a local reference with a warning", async () => {
  const ipc = uploadIpc(undefined, {
    pick: {
      value: "local:0123456789ab",
      name: "upload.bin",
      outside: true,
      missing: false,
    },
  });
  await openBody();
  await userEvent.click(await mode("Binary"));
  await userEvent.click(screen.getByRole("button", { name: "Choose…" }));
  expect(
    await screen.findByText(
      /upload.bin is outside the project, so it is not committed/,
    ),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  await waitFor(() =>
    expect(ipc.saved.at(-1)?.body).toEqual({
      mode: "binary",
      file: "local:0123456789ab",
    }),
  );
});

test("json_error_names_the_line", async () => {
  uploadIpc();
  await openBody();
  await userEvent.click(await mode("JSON"));
  await editor();
  expect(document.querySelector("p[aria-live='polite']")?.textContent).toBe("");
  await type('{\n  "a": 1\n  "b": 2\n}');
  await waitFor(() =>
    expect(
      document.querySelector("p[aria-live='polite']")?.textContent,
    ).toMatch(/^Line 3:/),
  );
});

test("format_button_formats_json", async () => {
  uploadIpc();
  await openBody();
  await userEvent.click(await mode("JSON"));
  const format = screen.getByRole("button", { name: /Format/ });
  expect(format).toBeDisabled();
  await type('{"a":1}');
  await waitFor(() => expect(format).toBeEnabled());
  await userEvent.click(format);
  expect((await editor()).state.doc.toString()).toBe('{\n  "a": 1\n}');
  await type("{");
  await waitFor(() => expect(format).toBeDisabled());
});

test("switching_modes_keeps_the_text_of_each_mode", async () => {
  const ipc = uploadIpc();
  await openBody();
  await userEvent.click(await mode("JSON"));
  await type('{"a":1}');
  await userEvent.click(await mode("Raw"));
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Raw language" }),
    "XML",
  );
  await type("<a/>");
  await userEvent.click(await mode("JSON"));
  expect((await editor()).state.doc.toString()).toBe('{"a":1}');
  await userEvent.click(await mode("Raw"));
  expect((await editor()).state.doc.toString()).toBe("<a/>");
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  await waitFor(() =>
    expect(ipc.saved.at(-1)?.body).toEqual({
      mode: "raw",
      language: "xml",
      text: "<a/>",
    }),
  );
  await userEvent.click(await mode("None"));
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  await waitFor(() => expect(ipc.saved).toHaveLength(2));
  expect(ipc.saved.at(-1)?.body).toBeUndefined();
});

test("form_data_rows_can_be_files", async () => {
  const ipc = uploadIpc(undefined, {
    pick: {
      value: "assets/avatar.png",
      name: "avatar.png",
      outside: false,
      missing: false,
    },
  });
  await openBody();
  await userEvent.click(await mode("Form data"));
  await userEvent.click(screen.getByRole("button", { name: "Add field" }));
  await userEvent.type(screen.getByRole("textbox", { name: /^Key/ }), "avatar");
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Type of avatar" }),
    "File",
  );
  await userEvent.click(screen.getByRole("button", { name: "Choose…" }));
  expect(await screen.findByText("avatar.png")).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Send avatar" })).toBeChecked();
  await userEvent.click(screen.getByRole("button", { name: "Save request" }));
  await waitFor(() =>
    expect(ipc.saved.at(-1)?.body).toEqual({
      mode: "formData",
      fields: [{ name: "avatar", file: "assets/avatar.png", enabled: true }],
    }),
  );
});

test("missing_local_file_asks_to_choose_again", async () => {
  uploadIpc(
    { mode: "binary", file: "local:0123456789ab" },
    { describe: { "local:0123456789ab": { missing: true, outside: true } } },
  );
  await openBody();
  expect(await screen.findByText("Missing")).toBeInTheDocument();
  expect(
    screen.getByText(
      "This file was picked on another device. Choose it again on this one.",
    ),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Choose…" })).toBeInTheDocument();
});

test("urlencoded_fields_count_in_the_tab_label", async () => {
  uploadIpc({
    mode: "urlencoded",
    fields: [
      { name: "a", value: "1" },
      { name: "b", value: "2", enabled: false },
      { name: "c", value: "3" },
    ],
  });
  openRequestTab(UPLOAD);
  expect(
    await screen.findByRole("tab", { name: "Body 2" }),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("tab", { name: "Body 2" }));
  expect(screen.getByRole("radio", { name: "URL encoded" })).toBeChecked();
  expect(screen.getByRole("button", { name: "Add field" })).toBeInTheDocument();
});
