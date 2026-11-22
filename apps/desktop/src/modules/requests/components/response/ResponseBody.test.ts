import { screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, onTestFinished, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import {
  openRequestTab,
  setUpRequestTabs,
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

beforeEach(() => {
  setUpRequestTabs();
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
  expect(input).toHaveAttribute("placeholder", "$.events[*].code");

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

test("a_narrow_pane_folds_the_filter_into_a_button", async () => {
  const { resize, pane } = await renderDone();
  resize(pane, 620, 600);
  expect(
    screen.queryByRole("textbox", { name: "Filter with JSONPath" }),
  ).toBeNull();
  await userEvent.click(
    screen.getByRole("button", { name: "Show the JSONPath filter" }),
  );
  expect(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
  ).toHaveFocus();
  expect(
    screen.getByRole("button", { name: "Hide the JSONPath filter" }),
  ).toHaveAttribute("aria-expanded", "true");
});

test("a_wide_pane_shows_the_filter_inline", async () => {
  const { resize, pane } = await renderDone();
  resize(pane, 800, 600);
  const row = screen.getByRole("tablist", {
    name: "Response sections",
  }).parentElement;
  expect(row).toContainElement(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
  );
  expect(row).not.toContainElement(
    within(screen.getByRole("region", { name: "Response" })).getByRole(
      "tabpanel",
    ),
  );
  expect(
    screen.queryByRole("button", { name: "Show the JSONPath filter" }),
  ).toBeNull();
});

test("a_hidden_filter_keeps_its_path_but_stops_filtering", async () => {
  const { resize, pane } = await renderDone({
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: () => ({ text: "[\n  42\n]", count: 1 }),
  });
  resize(pane, 620, 600);
  const body = () => screen.getByRole("textbox", { name: "Response body" });
  await waitFor(() => expect(body()).toHaveTextContent('"a": 1'));
  await userEvent.click(
    screen.getByRole("button", { name: "Show the JSONPath filter" }),
  );
  await userEvent.type(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
    "$.a",
  );
  await waitFor(() => expect(body()).toHaveTextContent("42"));
  await userEvent.click(
    screen.getByRole("button", { name: "Hide the JSONPath filter" }),
  );
  await waitFor(() => expect(body()).toHaveTextContent('"a": 1'));
  await userEvent.click(
    screen.getByRole("button", { name: "Show the JSONPath filter" }),
  );
  expect(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
  ).toHaveValue("$.a");
  await waitFor(() => expect(body()).toHaveTextContent("42"));
});

test("a_typed_filter_moves_to_its_own_row_when_the_pane_narrows", async () => {
  const { resize, pane } = await renderDone({
    text: { pretty: '{\n  "a": 1\n}', raw: '{"a":1}' },
    filter: () => ({ text: "[\n  42\n]", count: 1 }),
  });
  resize(pane, 800, 600);
  const body = () => screen.getByRole("textbox", { name: "Response body" });
  await waitFor(() => expect(body()).toHaveTextContent('"a": 1'));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
    "$.a",
  );
  await waitFor(() => expect(body()).toHaveTextContent("42"));
  // Narrower than the filter needs in the row: it opens a row of its own,
  // with the path, and goes on filtering.
  resize(pane, 620, 600);
  expect(
    screen.getByRole("button", { name: "Hide the JSONPath filter" }),
  ).toHaveAttribute("aria-expanded", "true");
  const row = screen.getByRole("tablist", {
    name: "Response sections",
  }).parentElement;
  const field = screen.getByRole("textbox", { name: "Filter with JSONPath" });
  expect(row).not.toContainElement(field);
  expect(field).toHaveValue("$.a");
  await waitFor(() => expect(body()).toHaveTextContent("42"));
  expect(screen.getByText("1 match")).toBeInTheDocument();
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
  await waitFor(() =>
    expect(screen.getByRole("radio", { name: "Raw" })).toBeChecked(),
  );
  expect(
    screen.getByRole("textbox", { name: "Filter with JSONPath" }),
  ).toHaveValue("$.a");
  expect(await screen.findByText("1 match")).toBeInTheDocument();
});
