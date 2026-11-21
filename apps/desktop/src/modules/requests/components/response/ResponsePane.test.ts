import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import {
  afterEach,
  beforeEach,
  expect,
  onTestFinished,
  test,
  vi,
} from "vitest";
import type { SendEvent } from "$lib/ipc/bindings/SendEvent";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import type { FakeProject } from "../../fake-project.svelte";
import { manifest } from "../../index";
import RequestTab from "../../RequestTab.svelte";
import {
  openRequestTab,
  setUpRequestTabs,
  type FakeShell,
} from "../../request-tab-support.svelte";
import {
  doneExchange,
  fakeRequestsIpc,
  fakeResizeObserver,
  responseInfo,
  type RequestsIpc,
} from "../../test-support";
import { sessionView } from "../../state/session-view.svelte";
import toolbarSource from "./ResponseToolbar.svelte?raw";

const PATH = "requests/shipments/list.yaml";
const file: RequestFile = {
  name: "List shipments",
  method: "GET",
  url: "https://api.test/v1/shipments",
};
const files = { [PATH]: file };

let shell: FakeShell;
let project: FakeProject;

beforeEach(() => {
  ({ shell, project } = setUpRequestTabs());
  settings.apply(DEFAULT_SETTINGS);
});

afterEach(() => {
  vi.useRealTimers();
  sessionView.timingLegend = false;
});

const send = async () =>
  userEvent.click(await screen.findByRole("button", { name: "Send" }));

const calls = (ipc: RequestsIpc, cmd: string) =>
  ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);

/** Opens the request in a work area `width` px wide. */
async function openAtWidth(width: number): Promise<void> {
  const observer = fakeResizeObserver();
  onTestFinished(observer.restore);
  openRequestTab(PATH);
  await screen.findByRole("region", { name: "Response" });
  observer.resize(document.querySelector(".split")!, width, 700);
}

/**
 * Sends the request and waits for `response`. `resize` gives an element a
 * size, as for `pane`, the response pane.
 */
async function renderDone(response: ResponseInfo): Promise<{
  resize: (element: Element, width: number, height: number) => void;
  pane: HTMLElement;
  id: string;
  unmount: () => void;
}> {
  const observer = fakeResizeObserver();
  onTestFinished(observer.restore);
  fakeRequestsIpc({
    files,
    send: { outcome: { type: "response", response } },
  });
  const { id, unmount } = openRequestTab(PATH);
  await send();
  await screen.findByRole("button", { name: /^Timing,/ });
  const pane = screen.getByRole("region", { name: "Response" });
  return { resize: observer.resize, pane, id, unmount };
}

/** Whether the response pane follows the editor in the page. */
function responseIsBelow(): boolean {
  return (
    document.querySelector('[data-layout="below"]') !== null &&
    document.querySelector('[data-layout="right"]') === null
  );
}

test("idle_pane_invites_a_send_with_the_keycap", async () => {
  fakeRequestsIpc({ files });
  openRequestTab(PATH);
  const pane = await screen.findByRole("region", { name: "Response" });
  expect(
    within(pane).getByText("Send the request to see the response here."),
  ).toBeInTheDocument();
  expect([...pane.querySelectorAll("kbd")].map((k) => k.textContent)).toEqual([
    "Ctrl",
    "Enter",
  ]);
});

test("done_toolbar_shows_status_timing_size_and_version", async () => {
  fakeRequestsIpc({
    files,
    send: doneExchange("json", {
      status: 404,
      reason: "Not Found",
      size: 1536,
    }),
  });
  openRequestTab(PATH);
  await send();
  const bar = await screen.findByRole("toolbar", { name: "Response" });
  expect(bar).toHaveAttribute("data-tone", "warning");
  expect(within(bar).getByText("404")).toBeInTheDocument();
  expect(within(bar).getByText("Not Found")).toBeInTheDocument();
  expect(within(bar).getByText("148 ms")).toBeInTheDocument();
  expect(within(bar).getByText("1.5 KB")).toBeInTheDocument();
  expect(within(bar).getByText("HTTP/1.1")).toBeInTheDocument();
  expect(
    within(bar).getByRole("button", { name: /^Timing, 148 ms total/ }),
  ).toBeInTheDocument();
});

test("the_status_code_and_reason_are_separate", async () => {
  await renderDone(responseInfo());
  const bar = screen.getByRole("toolbar", { name: "Response" });
  expect(bar.querySelector(".code")?.textContent).toBe("200");
  expect(bar.querySelector(".reason")?.textContent).toBe("OK");
});

test("the_timing_bar_opens_its_legend", async () => {
  await renderDone(
    responseInfo({
      timing: {
        dnsMs: 4,
        connectMs: 12,
        tlsMs: 18,
        waitingMs: 96,
        downloadMs: 18,
        totalMs: 148,
        reused: false,
      },
    }),
  );
  const bar = screen.getByRole("button", { name: /^Timing, 148 ms total/ });
  expect(bar).toHaveAttribute("aria-expanded", "false");
  expect(screen.queryByText("Waiting for server")).toBeNull();
  await userEvent.click(bar);
  expect(bar).toHaveAttribute("aria-expanded", "true");
  expect(screen.getByText("Waiting for server")).toBeInTheDocument();
});

test("the_legend_lists_every_phase_and_a_reused_connection", async () => {
  await renderDone(
    responseInfo({
      timing: {
        dnsMs: 0,
        connectMs: 0,
        tlsMs: 0,
        waitingMs: 96,
        downloadMs: 18,
        totalMs: 114,
        reused: true,
      },
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: /^Timing,/ }));
  const legend = within(screen.getByRole("list", { name: "Timing" }));
  expect(
    legend
      .getAllByRole("listitem")
      .map((item) => item.textContent?.replace(/\s+/g, " ").trim()),
  ).toEqual([
    "DNS 0 ms",
    "Connect 0 ms",
    "TLS 0 ms reused",
    "Waiting for server 96 ms",
    "Download 18 ms",
  ]);
});

test("a_narrow_legend_keeps_every_name_for_assistive_tech", async () => {
  await renderDone(
    responseInfo({
      timing: {
        dnsMs: 0,
        connectMs: 0,
        tlsMs: 0,
        waitingMs: 96,
        downloadMs: 18,
        totalMs: 114,
        reused: true,
      },
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: /^Timing,/ }));
  const items = within(
    screen.getByRole("list", { name: "Timing" }),
  ).getAllByRole("listitem");
  expect(items.map((item) => item.getAttribute("title"))).toEqual([
    "DNS 0 ms",
    "Connect 0 ms",
    "TLS 0 ms",
    "Waiting for server 96 ms",
    "Download 18 ms",
  ]);
  expect(items.map((item) => item.querySelector(".name")?.textContent)).toEqual(
    ["DNS", "Connect", "TLS", "Waiting for server", "Download"],
  );
  // Short of room, the names leave the screen but not the page.
  const compact =
    /@container \(width < \d+px\) \{[^@]*?\.name \{([^}]*)\}/.exec(
      toolbarSource,
    )?.[1] ?? "";
  expect(compact).toContain("clip-path: inset(50%)");
  expect(compact).not.toContain("display: none");
});

test("the_legend_ends_with_size_and_version", async () => {
  await renderDone(responseInfo({ size: 612, version: "HTTP/2" }));
  const bar = screen.getByRole("button", { name: /^Timing,/ });
  await userEvent.click(bar);
  const legend = document.getElementById(bar.getAttribute("aria-controls")!);
  expect(
    [...legend!.children].slice(-2).map((child) => child.textContent),
  ).toEqual(["612 B", "HTTP/2"]);
});

test("the_legend_choice_is_kept_for_the_session", async () => {
  const { id, unmount } = await renderDone(responseInfo());
  await userEvent.click(screen.getByRole("button", { name: /^Timing,/ }));
  unmount();
  expect(screen.queryByText("Waiting for server")).toBeNull();
  render(RequestTab, { props: { tab: shell.tab(id) } });
  expect(await screen.findByText("Waiting for server")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /^Timing,/ })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
});

test("the_environment_pill_always_shows", async () => {
  fakeRequestsIpc({ files });
  await project.setEnvironment("staging");
  openRequestTab(PATH);
  await send();
  await screen.findByRole("button", { name: /^Timing,/ });
  const bar = screen.getByRole("toolbar", { name: "Response" });
  expect(within(bar).getByText("staging")).toBeInTheDocument();
  expect(within(bar).queryByText(/^Sent in/)).toBeNull();
});

test("sending_names_its_environment", async () => {
  fakeRequestsIpc({ files, send: { hold: true } });
  await project.setEnvironment("staging");
  openRequestTab(PATH);
  await send();
  const bar = await screen.findByRole("toolbar", { name: "Response" });
  expect(within(bar).getByText("Sending")).toBeInTheDocument();
  expect(within(bar).getByText("staging")).toBeInTheDocument();
});

/**
 * Whether `element` is in an inert part of the page. jsdom knows nothing of
 * `inert`: the property Svelte sets on the element is all there is to see.
 */
function isInert(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) return false;
  return element.inert || isInert(element.parentElement);
}

test("the_second_row_stays_while_sending", async () => {
  const ipc = fakeRequestsIpc({ files });
  openRequestTab(PATH);
  await send();
  await screen.findByRole("button", { name: /^Timing,/ });
  const sections = () =>
    screen.getByRole("tablist", { name: "Response sections" });
  expect(isInert(sections())).toBe(false);
  ipc.send = { ...ipc.send, hold: true };
  await send();
  const pane = screen.getByRole("region", { name: "Response" });
  await waitFor(() => expect(pane).toHaveAttribute("aria-busy", "true"));
  expect(isInert(sections())).toBe(true);
});

test("sent_in_shows_only_when_the_environment_changed", async () => {
  fakeRequestsIpc({ files });
  await project.setEnvironment("local");
  openRequestTab(PATH);
  await send();
  await screen.findByRole("toolbar", { name: "Response" });
  expect(screen.queryByText(/^Sent in/)).not.toBeInTheDocument();
  await project.setEnvironment("staging");
  expect(await screen.findByText("Sent in local")).toBeInTheDocument();
});

test("sending_view_lists_phases_as_they_arrive", async () => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
  const ipc = fakeRequestsIpc({
    files,
    send: {
      hold: true,
      events: [
        {
          type: "started",
          method: "GET",
          url: "https://api.test/v1/shipments",
        },
        { type: "phase", phase: "dns", ms: 4 },
        { type: "phase", phase: "connect", ms: 12 },
        { type: "phase", phase: "tls", ms: 18 },
      ],
    },
  });
  openRequestTab(PATH);
  const button = await screen.findByRole("button", { name: "Send" });
  button.click();
  const pane = await screen.findByRole("region", { name: "Response" });
  await waitFor(() => expect(pane).toHaveAttribute("aria-busy", "true"));
  expect(within(pane).getByText("Sending")).toBeInTheDocument();
  expect(within(pane).getByText("DNS lookup 4 ms")).toBeInTheDocument();
  expect(within(pane).getByText("Connect 12 ms")).toBeInTheDocument();
  expect(within(pane).getByText("TLS 18 ms")).toBeInTheDocument();
  expect(within(pane).getByText(/^Waiting for api\.test/)).toBeInTheDocument();
  expect(
    within(pane).getByRole("progressbar", { name: "Waiting for the server" }),
  ).toBeInTheDocument();
  expect(within(pane).getByText("0.0 s")).toBeInTheDocument();
  vi.advanceTimersByTime(1200);
  await waitFor(() =>
    expect(within(pane).getByText("1.2 s")).toBeInTheDocument(),
  );
  ipc.finishSends();
  await waitFor(() => expect(pane).not.toHaveAttribute("aria-busy", "true"));
  // The timer stopped with the send.
  expect(vi.getTimerCount()).toBe(0);
});

test("sending_view_moves_on_to_downloading_after_the_head", async () => {
  fakeRequestsIpc({
    files,
    send: {
      hold: true,
      events: [
        {
          type: "started",
          method: "GET",
          url: "https://api.test/v1/shipments",
        },
        { type: "reused" },
        { type: "head", status: 200, reason: "OK", headers: [] },
        { type: "body", received: 2048, text: null },
      ],
    },
  });
  openRequestTab(PATH);
  await send();
  const pane = await screen.findByRole("region", { name: "Response" });
  expect(
    await within(pane).findByText("Reused connection"),
  ).toBeInTheDocument();
  expect(within(pane).getByText("Downloading 2.0 KB")).toBeInTheDocument();
  expect(
    within(pane).getByRole("progressbar", { name: "Downloading" }),
  ).toBeInTheDocument();
  expect(within(pane).queryByText(/^Waiting for/)).not.toBeInTheDocument();
});

test("waiting_names_the_server_until_the_host_is_known", async () => {
  fakeRequestsIpc({ files, send: { hold: true } });
  openRequestTab(PATH);
  await send();
  const pane = await screen.findByRole("region", { name: "Response" });
  expect(
    await within(pane).findByText(/^Waiting for the server /),
  ).toBeInTheDocument();
});

test("failure_shows_title_message_and_code", async () => {
  fakeRequestsIpc({
    files,
    send: {
      outcome: {
        type: "failed",
        failure: {
          kind: "dns",
          title: "Could not find the server",
          message: "api.test did not resolve to an address.",
          code: "ENOTFOUND",
        },
      },
    },
  });
  openRequestTab(PATH);
  await send();
  const card = await screen.findByRole("alert");
  expect(
    within(card).getByText("Could not find the server"),
  ).toBeInTheDocument();
  expect(
    within(card).getByText("api.test did not resolve to an address."),
  ).toBeInTheDocument();
  expect(within(card).getByText("ENOTFOUND")).toBeInTheDocument();
});

test("cancelled_after_head_says_the_body_is_incomplete", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: {
      hold: true,
      events: [
        {
          type: "started",
          method: "GET",
          url: "https://api.test/v1/shipments",
        },
        { type: "head", status: 200, reason: "OK", headers: [] },
      ],
    },
  });
  openRequestTab(PATH);
  await send();
  await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  expect(calls(ipc, "cancel_send")).toHaveLength(1);
  const pane = await screen.findByRole("region", { name: "Response" });
  expect(
    await within(pane).findByText(/^Cancelled after \d+\.\d s\.$/),
  ).toBeInTheDocument();
  expect(
    within(pane).getByText("200 OK. The body was not received in full."),
  ).toBeInTheDocument();
});

test("cancelled_before_the_head_only_says_how_long", async () => {
  fakeRequestsIpc({ files, send: { hold: true } });
  openRequestTab(PATH);
  await send();
  await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  const pane = await screen.findByRole("region", { name: "Response" });
  expect(await within(pane).findByText(/^Cancelled after/)).toBeInTheDocument();
  expect(
    within(pane).queryByText(/not received in full/),
  ).not.toBeInTheDocument();
});

test("layout_setting_moves_the_response_below", async () => {
  fakeRequestsIpc({ files });
  await openAtWidth(1200);
  expect(responseIsBelow()).toBe(false);
  settings.apply({
    ...DEFAULT_SETTINGS,
    requests: { ...DEFAULT_SETTINGS.requests, responseLayout: "below" },
  });
  await waitFor(() => expect(responseIsBelow()).toBe(true));
});

test("a_narrow_window_always_stacks", async () => {
  fakeRequestsIpc({ files });
  // Under 760 px of work area, whatever the window.
  await openAtWidth(759);
  expect(settings.requests.responseLayout).toBe("right");
  expect(responseIsBelow()).toBe(true);
  // Stacked, the response cannot move, so the toolbar does not offer it.
  await send();
  const bar = await screen.findByRole("toolbar", { name: "Response" });
  expect(
    within(bar).queryByRole("button", { name: /^Show response/ }),
  ).toBeNull();
});

test("the_toolbar_button_moves_the_response", async () => {
  const ipc = fakeRequestsIpc({ files });
  openRequestTab(PATH);
  void ipc;
  await send();
  const bar = await screen.findByRole("toolbar", { name: "Response" });
  await userEvent.click(
    within(bar).getByRole("button", { name: "Show response below" }),
  );
  await waitFor(() => expect(settings.requests.responseLayout).toBe("below"));
  expect(
    await within(bar).findByRole("button", {
      name: "Show response on the right",
    }),
  ).toBeInTheDocument();
});

test("toggle_layout_is_a_command", () => {
  expect(manifest.commands).toContainEqual({
    id: "requests.toggleLayout",
    title: "Toggle response position",
    action: { type: "intent", intent: "requests.toggleLayout" },
  });
  expect(manifest.intents?.map((i) => i.intent)).toContain(
    "requests.toggleLayout",
  );
});

test("copy_is_off_for_large_bodies", async () => {
  fakeRequestsIpc({ files, send: doneExchange("json", { large: true }) });
  openRequestTab(PATH);
  await send();
  const copy = await screen.findByRole("button", { name: "Copy response" });
  expect(copy).toBeDisabled();
  expect(copy).toHaveAttribute(
    "title",
    "Too large to copy. Save it to a file instead.",
  );
  expect(
    screen.getByRole("button", { name: "Save response to file" }),
  ).toBeEnabled();
});

test("copy_is_off_for_binary_bodies", async () => {
  fakeRequestsIpc({ files, send: doneExchange("binary") });
  openRequestTab(PATH);
  await send();
  const copy = await screen.findByRole("button", { name: "Copy response" });
  expect(copy).toBeDisabled();
  expect(copy).toHaveAttribute(
    "title",
    "A binary body cannot be copied as text.",
  );
});

test("copy_and_save_report_with_a_toast", async () => {
  const ipc = fakeRequestsIpc({ files });
  const { id } = openRequestTab(PATH, "List shipments");
  await send();
  await userEvent.click(
    await screen.findByRole("button", { name: "Copy response" }),
  );
  await waitFor(() => expect(shell.toasts).toContain("Response copied"));
  expect(calls(ipc, "copy_response")).toEqual([{ id }]);
  await userEvent.click(
    screen.getByRole("button", { name: "Save response to file" }),
  );
  await waitFor(() =>
    expect(shell.toasts).toContain("Saved list-shipments.json"),
  );
  expect(ipc.savedResponses).toEqual([{ id, name: "list-shipments.json" }]);
});

test("save_is_off_for_an_empty_body", async () => {
  fakeRequestsIpc({ files, send: doneExchange("empty", { size: 0 }) });
  openRequestTab(PATH);
  await send();
  expect(
    await screen.findByRole("button", { name: "Save response to file" }),
  ).toBeDisabled();
});

test("production_environment_pill_keeps_its_mark", async () => {
  fakeRequestsIpc({
    files,
    send: { outcome: { type: "response", response: responseInfo() } },
  });
  project.info = {
    ...project.info!,
    environments: [
      { name: "local", production: false },
      { name: "live", production: true },
      { name: "staging", production: false },
    ],
  };
  await project.setEnvironment("live");
  openRequestTab(PATH);
  await send();
  await screen.findByRole("toolbar", { name: "Response" });
  await project.setEnvironment("staging");
  expect(await screen.findByText("Sent in live")).toHaveClass("env-prod");
});

const started: SendEvent = {
  type: "started",
  method: "GET",
  url: "https://api.test/v1/shipments",
};
const sse: SendEvent = {
  type: "head",
  status: 200,
  reason: "OK",
  headers: [{ name: "Content-Type", value: "text/event-stream" }],
};

test("chunked_text_shows_in_the_pane_while_sending", async () => {
  fakeRequestsIpc({
    files,
    send: {
      hold: true,
      events: [
        started,
        { type: "head", status: 200, reason: "OK", headers: [] },
        { type: "body", received: 6, text: "part 1" },
        { type: "body", received: 12, text: ", part2" },
      ],
    },
  });
  openRequestTab(PATH);
  await send();
  const pane = await screen.findByRole("region", { name: "Response" });
  expect(await within(pane).findByText("part 1, part2")).toBeInTheDocument();
  expect(within(pane).getByText("Receiving… 12 B")).toBeInTheDocument();
});

test("events_list_live_while_sending_and_survive_a_cancel", async () => {
  fakeRequestsIpc({
    files,
    send: {
      hold: true,
      events: [
        started,
        sse,
        { type: "event", atMs: 5, event: "tick", data: "one", id: null },
        { type: "event", atMs: 9, event: null, data: "two", id: null },
      ],
    },
  });
  openRequestTab(PATH);
  await send();
  const pane = await screen.findByRole("region", { name: "Response" });
  const list = () =>
    within(pane).getByRole("list", { name: "Server-sent events" });
  await waitFor(() =>
    expect(within(list()).getAllByRole("listitem")).toHaveLength(2),
  );
  await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
  await within(pane).findByText(/^Cancelled after/);
  expect(within(list()).getAllByRole("listitem")).toHaveLength(2);
  expect(within(pane).getByText("2 events")).toBeInTheDocument();
});

test("a_finished_stream_keeps_its_event_list", async () => {
  fakeRequestsIpc({
    files,
    send: {
      ...doneExchange("events"),
      events: [
        started,
        sse,
        { type: "event", atMs: 5, event: "tick", data: "one", id: null },
      ],
    },
  });
  openRequestTab(PATH);
  await send();
  const list = await screen.findByRole("list", { name: "Server-sent events" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  expect(screen.getByText("tick")).toBeInTheDocument();
});
