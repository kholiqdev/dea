import { screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, onTestFinished, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import {
  openRequestTab,
  setUpRequestTabs,
  type FakeShell,
} from "../../request-tab-support.svelte";
import {
  doneExchange,
  fakeRequestsIpc,
  fakeResizeObserver,
  fakeResponse,
  fakeRows,
  type RequestsIpc,
} from "../../test-support";
import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
import type { BodyKind } from "$lib/ipc/bindings/BodyKind";
import sendActiveTab from "../../intents/send";
import filterSource from "./JsonFilter.svelte?raw";
import {
  FILTER_ACTIVE_WIDTH,
  FILTER_IDLE_WIDTH,
  FILTER_MIN_WIDTH,
} from "./filter-layout";
import paneSource from "./ResponsePane.svelte?raw";

const PATH = "requests/shipments/list.yaml";
const file: RequestFile = {
  name: "List shipments",
  method: "GET",
  url: "https://api.test/v1/shipments",
};
const files = { [PATH]: file };

const tab = (name: string | RegExp) =>
  within(screen.getByRole("tablist", { name: "Response sections" })).getByRole(
    "tab",
    { name },
  );

let shell: FakeShell;
beforeEach(() => {
  ({ shell } = setUpRequestTabs());
  settings.apply(DEFAULT_SETTINGS);
});

async function open(
  kind: BodyKind,
  response: ReturnType<typeof fakeResponse> = {},
  over: Partial<ResponseInfo> = {},
): Promise<RequestsIpc> {
  const ipc = fakeRequestsIpc({
    files,
    send: doneExchange(kind, over),
    response,
  });
  openRequestTab(PATH);
  await userEvent.click(await screen.findByRole("button", { name: "Send" }));
  await screen.findByRole("tablist", { name: "Response sections" });
  return ipc;
}

/**
 * Sends the request and waits for a JSON response. `resize` gives an
 * element a size, as for `pane`, the response pane.
 */
async function renderDone(
  response: ReturnType<typeof fakeResponse> = {},
): Promise<{
  resize: (element: Element, width: number, height: number) => void;
  pane: HTMLElement;
}> {
  const observer = fakeResizeObserver();
  onTestFinished(observer.restore);
  await open("json", response);
  const pane = screen.getByRole("region", { name: "Response" });
  return { resize: observer.resize, pane };
}

const views = () =>
  within(screen.getByRole("radiogroup", { name: "Body view" }))
    .getAllByRole("radio")
    .map((r) => [r.textContent?.trim(), r.getAttribute("aria-checked")]);

test("json_opens_pretty_with_folding", async () => {
  await open("json", {
    text: { pretty: '{\n  "id": 1\n}', raw: '{"id":1}' },
  });
  expect(
    await screen.findByRole("textbox", { name: "Response body" }),
  ).toHaveTextContent('"id": 1');
  expect(document.querySelector(".cm-foldGutter")).not.toBeNull();
});

test("raw_shows_the_body_as_received", async () => {
  await open("json", { text: { pretty: '{\n  "id": 1\n}', raw: '{"id":1}' } });
  await screen.findByRole("textbox", { name: "Response body" });
  await userEvent.click(screen.getByRole("radio", { name: "Raw" }));
  await waitFor(() =>
    expect(
      screen.getByRole("textbox", { name: "Response body" }),
    ).toHaveTextContent('{"id":1}'),
  );
  expect(document.querySelector(".cm-foldGutter")).toBeNull();
});

test("the_chosen_view_stays_while_the_tabs_change", async () => {
  await open("json", { text: { pretty: "{\n}", raw: "{}" } });
  await userEvent.click(await screen.findByRole("radio", { name: "Raw" }));
  await userEvent.click(tab(/^Headers/));
  await userEvent.click(tab("Body"));
  expect(screen.getByRole("radio", { name: "Raw" })).toBeChecked();
});

test("views_follow_the_body_kind", async () => {
  await open("json");
  expect(views()).toEqual([
    ["Pretty", "true"],
    ["Raw", "false"],
  ]);
});

test("xml_has_pretty_and_raw", async () => {
  await open("xml");
  expect(views()).toEqual([
    ["Pretty", "true"],
    ["Raw", "false"],
  ]);
  expect(
    screen.queryByRole("textbox", { name: "Filter with JSONPath" }),
  ).toBeNull();
});

test("html_adds_preview", async () => {
  await open("html");
  expect(views()).toEqual([
    ["Pretty", "true"],
    ["Raw", "false"],
    ["Preview", "false"],
  ]);
});

test("image_and_pdf_open_on_preview", async () => {
  await open("image");
  // One view leaves nothing to choose.
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
});

test("pdf_opens_on_preview", async () => {
  await open("pdf");
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
});

test("text_is_raw_only", async () => {
  await open("text", { text: { raw: "plain" } });
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
  expect(
    await screen.findByRole("textbox", { name: "Response body" }),
  ).toHaveTextContent("plain");
});

test("binary_says_to_save_it", async () => {
  await open("binary", {}, { size: 2048 });
  expect(
    screen.getByText("Binary body, 2.0 KB. Save it to a file to open it."),
  ).toBeInTheDocument();
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
});

test("empty_says_there_is_no_body", async () => {
  await open("empty");
  expect(screen.getByText("No body")).toBeInTheDocument();
});

test("a_large_body_opens_in_the_row_viewer", async () => {
  await open("json", { rows: fakeRows({ total: 3 }) }, { large: true });
  expect(await screen.findByText("row 2")).toBeInTheDocument();
  expect(screen.getByRole("document", { name: "Response body" })).toBeVisible();
  expect(
    screen.queryByRole("textbox", { name: "Filter with JSONPath" }),
  ).toBeNull();
});

test("an_event_stream_opens_in_the_event_list", async () => {
  await open("events");
  expect(
    screen.getByRole("list", { name: "Server-sent events" }),
  ).toBeInTheDocument();
});

test("not_valid_json_says_it_is_shown_as_received", async () => {
  await open("json", {
    text: { pretty: "{oops", raw: "{oops", formatted: false },
  });
  expect(
    await screen.findByText("Not valid JSON, shown as received."),
  ).toBeInTheDocument();
});

test("filter_shows_matches_and_errors", async () => {
  await open("json", {
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: (path) =>
      path === "$.a"
        ? { text: "[\n  1\n]", count: 1 }
        : path === "$.b[*]"
          ? { text: "[\n  1,\n  2\n]", count: 2 }
          : path === "$.none"
            ? { text: "[]", count: 0 }
            : { kind: "invalid", message: "unexpected token" },
  });
  await screen.findByRole("textbox", { name: "Response body" });
  const input = screen.getByRole("textbox", { name: "Filter with JSONPath" });

  await userEvent.type(input, "$.a");
  expect(await screen.findByText("1 match")).toBeInTheDocument();
  await waitFor(() =>
    expect(
      screen.getByRole("textbox", { name: "Response body" }),
    ).toHaveTextContent("1"),
  );

  await userEvent.clear(input);
  await userEvent.type(input, "$.b[[*]");
  expect(await screen.findByText("2 matches")).toBeInTheDocument();

  await userEvent.clear(input);
  await userEvent.type(input, "$.none");
  expect(await screen.findByText("No matches")).toBeInTheDocument();

  await userEvent.clear(input);
  await userEvent.type(input, "$.[[");
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Not a valid JSONPath: unexpected token",
  );

  await userEvent.keyboard("{Escape}");
  expect(input).toHaveValue("");
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(
    screen.getByRole("textbox", { name: "Response body" }),
  ).toHaveTextContent('"a": 1');
});

test("filter_waits_for_a_pause_in_typing", async () => {
  const ipc = await open("json", { text: { pretty: "{}", raw: "{}" } });
  await screen.findByRole("textbox", { name: "Response body" });
  const input = screen.getByRole("textbox", { name: "Filter with JSONPath" });
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    await userEvent.type(input, "$.a", { delay: null });
    const asked = () => ipc.calls.filter(([c]) => c === "filter_response");
    expect(asked()).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(250);
    expect(asked()).toHaveLength(1);
  } finally {
    vi.useRealTimers();
  }
});

test("a_body_that_cannot_be_read_says_so", async () => {
  await open("json", {
    textError: { kind: "missing", message: "the response is gone" },
  });
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The body could not be read: The response is gone",
  );
});

test("body_views_move_with_the_arrow_keys", async () => {
  await open("html", { text: { pretty: "<p></p>", raw: "<p></p>" } });
  const radio = (name: string) => screen.getByRole("radio", { name });
  expect(radio("Pretty")).toHaveAttribute("tabindex", "0");
  expect(radio("Raw")).toHaveAttribute("tabindex", "-1");
  radio("Pretty").focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(radio("Raw")).toHaveFocus();
  expect(radio("Raw")).toBeChecked();
  await userEvent.keyboard("{End}");
  expect(radio("Preview")).toBeChecked();
  await userEvent.keyboard("{ArrowRight}");
  expect(radio("Pretty")).toBeChecked();
  await userEvent.keyboard("{ArrowLeft}");
  expect(radio("Preview")).toBeChecked();
  await userEvent.keyboard("{Home}");
  expect(radio("Pretty")).toHaveFocus();
});

test("a_filter_that_cannot_run_is_not_called_invalid", async () => {
  await open("json", {
    text: { pretty: "{}", raw: "{}" },
    filter: () => ({ kind: "missing", message: "no response here" }),
  });
  await screen.findByRole("textbox", { name: "Response body" });
  await userEvent.type(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
    "$.a",
  );
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("The filter could not run: No response here");
  expect(alert).not.toHaveTextContent("Not a valid JSONPath");
});

test("a_declined_send_keeps_the_view_and_the_filter", async () => {
  const ipc = await open("json", {
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: () => ({ text: "[\n  1\n]", count: 1 }),
  });
  await screen.findByRole("textbox", { name: "Response body" });
  await userEvent.click(screen.getByRole("radio", { name: "Raw" }));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
    "$.a",
  );
  expect(await screen.findByText("1 match")).toBeInTheDocument();
  // Declined at the production prompt: the same response comes back.
  ipc.send = { ...ipc.send, outcome: { type: "notConfirmed" } };
  await userEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(ipc.sends).toHaveLength(2));
  const field = () =>
    screen.getByRole("textbox", { name: "Filter with JSONPath" });
  await waitFor(() => expect(field()).toHaveValue("$.a"));
  expect(await screen.findByText("1 match")).toBeInTheDocument();
  // The path hides the views; cleared and left, they come back as chosen.
  await userEvent.click(field());
  await userEvent.keyboard("{Escape}");
  await userEvent.tab();
  expect(screen.getByRole("radio", { name: "Raw" })).toBeChecked();
});

test("a_field_removed_while_focused_gives_the_views_back", async () => {
  const ipc = await open("json", { text: { pretty: "{}", raw: "{}" } });
  await userEvent.click(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
  );
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
  // Sent from the keyboard and declined: the field goes, with the focus in
  // it, and the same response comes back.
  ipc.send = { ...ipc.send, outcome: { type: "notConfirmed" } };
  shell.activateTab("tab-1");
  await sendActiveTab();
  await waitFor(() => expect(ipc.sends).toHaveLength(2));
  expect(
    await screen.findByRole("radiogroup", { name: "Body view" }),
  ).toBeInTheDocument();
});

test("the_filter_shows_at_every_default_width", async () => {
  const { resize, pane } = await renderDone({
    text: { pretty: "{}", raw: "{}" },
  });
  for (const width of [620, 543, 530]) {
    resize(pane, width, 600);
    expect(
      screen.getByRole("textbox", { name: "Filter with JSONPath" }),
    ).toBeVisible();
  }
  expect(screen.queryByRole("button", { name: /JSONPath filter/ })).toBeNull();
});

test("a_path_keeps_the_views_hidden_after_focus_leaves", async () => {
  await renderDone({
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: () => ({ text: "[\n  42\n]", count: 1 }),
  });
  await userEvent.click(screen.getByRole("radio", { name: "Raw" }));
  const field = screen.getByRole("textbox", { name: "Filter with JSONPath" });
  await userEvent.click(field);
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
  await userEvent.type(field, "$.a");
  await userEvent.tab();
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
  await userEvent.click(field);
  await userEvent.keyboard("{Escape}");
  await userEvent.tab();
  expect(screen.getByRole("radio", { name: "Raw" })).toBeChecked();
});

test("the_filter_has_a_neutral_placeholder_and_keycaps", async () => {
  await open("json");
  const field = screen.getByRole("textbox", { name: "Filter with JSONPath" });
  expect(field).toHaveAttribute("placeholder", "$.data[*].id");
  // jsdom is not macOS.
  expect(
    [...field.parentElement!.querySelectorAll("kbd")].map((k) => k.textContent),
  ).toEqual(["Ctrl", "Alt", "F"]);
  expect(field).toHaveAttribute("aria-keyshortcuts", "Control+Alt+F");
});

test("the_count_takes_the_place_of_the_keycaps", async () => {
  await open("json", {
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: () => ({ text: "[\n  1\n]", count: 1 }),
  });
  const field = screen.getByRole("textbox", { name: "Filter with JSONPath" });
  await userEvent.type(field, "$.a");
  await userEvent.tab();
  const count = await waitFor(() => {
    const found = field.parentElement?.querySelector(".count");
    expect(found).toHaveTextContent("1 match");
    return found;
  });
  expect(count).toHaveAttribute("aria-live", "polite");
  expect(field.parentElement?.querySelector("kbd")).toBeNull();
});

test("a_narrow_pane_shows_the_views_as_a_menu", async () => {
  const { resize, pane } = await renderDone({
    text: { pretty: '{\n  "id": 1\n}', raw: '{"id":1}' },
  });
  // jsdom measures nothing else: the sections count 0 px, and the views
  // their fallback widths.
  resize(pane, 300, 600);
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
  await userEvent.click(
    screen.getByRole("button", { name: "Body view: Pretty" }),
  );
  await userEvent.click(screen.getByRole("menuitemradio", { name: "Raw" }));
  await waitFor(() =>
    expect(
      screen.getByRole("textbox", { name: "Response body" }),
    ).toHaveTextContent('{"id":1}'),
  );
  expect(
    screen.getByRole("button", { name: "Body view: Raw" }),
  ).toBeInTheDocument();
});

test("the_path_stays_when_the_section_changes", async () => {
  await open("json", {
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: () => ({ text: "[\n  42\n]", count: 1 }),
  });
  const body = () => screen.getByRole("textbox", { name: "Response body" });
  const field = () =>
    screen.getByRole("textbox", { name: "Filter with JSONPath" });
  await waitFor(() => expect(body()).toHaveTextContent('"a": 1'));
  await userEvent.type(field(), "$.a");
  await waitFor(() => expect(body()).toHaveTextContent("42"));
  await userEvent.click(tab(/^Headers/));
  await userEvent.click(tab("Body"));
  expect(field()).toHaveValue("$.a");
  await waitFor(() => expect(body()).toHaveTextContent("42"));
});

/** The declarations of the top-level rule for `selector` in `source`. */
function rule(source: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`\\n  ${escaped} \\{([^}]*)\\}`).exec(source)?.[1] ?? "";
}

test("the_field_gives_way_before_the_sections", () => {
  // jsdom lays nothing out, so the rules are read from the source. A
  // shrink factor of 1000 takes the room from the field first, down to
  // 120 px; only then do the sections give way, and scroll.
  expect(rule(filterSource, ".field")).toContain(
    `flex: 0 1000 ${FILTER_IDLE_WIDTH}px;`,
  );
  expect(rule(filterSource, ".field")).toContain(
    `min-width: ${FILTER_MIN_WIDTH}px;`,
  );
  expect(rule(filterSource, ".field.active")).toContain(
    `flex: 0 1000 ${FILTER_ACTIVE_WIDTH}px;`,
  );
  const sections = rule(paneSource, ".sections");
  expect(sections).toContain("flex: 0 1 auto;");
  expect(sections).toContain("min-width: 0;");
});

test("the_filter_shortcut_opens_the_body_and_focuses_the_field", async () => {
  await renderDone({ text: { pretty: "{}", raw: "{}" } });
  shell.activateTab(shell.findTab("requests.request") ?? "");
  await userEvent.click(tab(/Headers/));
  const { default: run } = await import("../../intents/focus-filter");
  await run();
  expect(tab("Body")).toHaveAttribute("aria-selected", "true");
  expect(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
  ).toHaveFocus();
});

test("the_filter_shortcut_without_json_says_so", async () => {
  const { default: run } = await import("../../intents/focus-filter");
  await open("html");
  shell.activateTab(shell.findTab("requests.request") ?? "");
  await run();
  expect(shell.toasts).toEqual(["There is no JSON response to filter."]);
  // A tab that has sent nothing has no response either.
  const idle = openRequestTab("requests/shipments/other.yaml");
  shell.activateTab(idle.id);
  await run();
  expect(shell.toasts).toEqual([
    "There is no JSON response to filter.",
    "There is no JSON response to filter.",
  ]);
});

test("the_filter_shortcut_outside_a_request_tab_says_so", async () => {
  const { default: run } = await import("../../intents/focus-filter");
  const folder = shell.openTab("requests.folder", "Shipments", {
    path: "requests/shipments",
  });
  shell.activateTab(folder);
  await run();
  expect(shell.toasts).toEqual(["Open a request to filter its response."]);
});
