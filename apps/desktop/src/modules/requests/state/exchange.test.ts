import type { Channel } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { SendEvent } from "$lib/ipc/bindings/SendEvent";
import type { SendOutcome } from "$lib/ipc/bindings/SendOutcome";
import type { FakeProject } from "../fake-project.svelte";
import {
  setUpRequestTabs,
  useProduction,
  type FakeShell,
} from "../request-tab-support.svelte";
import {
  fakeRequestsIpc,
  responseInfo,
  type FakeInspection,
  type RequestsIpc,
} from "../test-support";
import {
  draftFor,
  INSPECT_DELAY,
  pruneDrafts,
  type RequestDraft,
} from "./drafts.svelte";
import {
  cookiesVersion,
  exchangeFor,
  exchanges,
  MAX_EVENTS,
  SECOND_SEND_GAP_MS,
  type SendResult,
} from "./exchange.svelte";

const PATH = "requests/shipments/list.yaml";
const file: RequestFile = {
  name: "List shipments",
  method: "GET",
  url: "{{baseUrl}}/v1/shipments",
};
const files = { [PATH]: file };
const ok200: SendOutcome = { type: "response", response: responseInfo() };

let shell: FakeShell;
let project: FakeProject;

beforeEach(() => {
  ({ shell, project } = setUpRequestTabs());
});

afterEach(() => {
  vi.useRealTimers();
});

/** The draft of an open request tab `id`, read from `path`. */
async function openDraft(id: string, path: string): Promise<RequestDraft> {
  shell.tabs.push({
    id,
    tabType: "requests.request",
    title: "Request",
    params: { path },
    dirty: false,
  });
  const draft = draftFor(id, path);
  await vi.waitFor(() => expect(draft.saved).not.toBeNull());
  return draft;
}

const calls = (ipc: RequestsIpc, cmd: string) =>
  ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);

/** Starts a send and waits until it is out; `sent` settles when it ends. */
async function startSend(
  draft: RequestDraft,
): Promise<{ sent: Promise<SendResult> }> {
  const sent = exchangeFor("t1").send(draft);
  await vi.waitFor(() => expect(exchangeFor("t1").state.phase).toBe("sending"));
  return { sent };
}

test("send_posts_the_draft_with_the_tab_id", async () => {
  const ipc = fakeRequestsIpc({ files, send: { outcome: ok200 } });
  const draft = await openDraft("t1", PATH);
  draft.typeUrl("{{baseUrl}}/v1/unsaved");
  await exchangeFor("t1").send(draft);
  expect(ipc.sends[0]).toMatchObject({
    id: "t1",
    path: PATH,
    request: { url: "{{baseUrl}}/v1/unsaved" },
  });
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "done",
    response: { status: 200, size: 2 },
  });
  // Sending does not save.
  expect(ipc.saved).toEqual([]);
});

test("a_second_send_while_sending_is_ignored", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const draft = await openDraft("t1", PATH);
  // Pressed twice before the first one is out, then again while it runs.
  const first = exchangeFor("t1").send(draft);
  await exchangeFor("t1").send(draft);
  await vi.waitFor(() => expect(exchangeFor("t1").state.phase).toBe("sending"));
  await exchangeFor("t1").send(draft);
  ipc.finishSends();
  await first;
  expect(ipc.sends).toHaveLength(1);
  expect(exchangeFor("t1").state.phase).toBe("done");
});

test("undefined_variables_warn_before_sending", async () => {
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["carrierCode"] },
  });
  const draft = await openDraft("t1", PATH);
  await exchangeFor("t1").send(draft);
  expect(calls(ipc, "send_request")).toEqual([]);
  expect(exchangeFor("t1").warning).toEqual(["carrierCode"]);
  expect(exchangeFor("t1").state.phase).toBe("idle");
});

test("send_anyway_sends", async () => {
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["carrierCode"] },
  });
  const draft = await openDraft("t1", PATH);
  await exchangeFor("t1").send(draft);
  await exchangeFor("t1").send(draft, { anyway: true });
  expect(ipc.sends).toHaveLength(1);
  expect(exchangeFor("t1").warning).toBeNull();
  expect(exchangeFor("t1").state.phase).toBe("done");
});

test("production_skips_the_banner", async () => {
  // The confirmation names them instead.
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["traceId"] },
  });
  await useProduction(project);
  const draft = await openDraft("t1", PATH);
  expect(await exchangeFor("t1").send(draft)).toBe("sent");
  expect(ipc.sends).toHaveLength(1);
  expect(exchangeFor("t1").warning).toBeNull();
});

test("send_reports_how_it_ended", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: { hold: true },
    inspection: (_path, request) => ({
      undefined: request.url.includes("{{traceId}}") ? ["traceId"] : [],
    }),
  });
  const draft = await openDraft("t1", PATH);
  const exchange = exchangeFor("t1");
  const first = exchange.send(draft);
  expect(await exchange.send(draft)).toBe("ignored");
  await vi.waitFor(() => expect(ipc.sends).toHaveLength(1));
  expect(await exchange.send(draft)).toBe("ignored");
  ipc.finishSends(ok200);
  expect(await first).toBe("sent");
  // Sent, whatever came of it.
  ipc.send = {
    ...ipc.send,
    hold: false,
    outcome: {
      type: "failed",
      failure: { kind: "refused", title: "Refused", message: "", code: null },
    },
  };
  expect(await exchange.send(draft)).toBe("sent");
  ipc.send.outcome = { type: "notConfirmed" };
  expect(await exchange.send(draft)).toBe("notConfirmed");
  draft.typeUrl("{{baseUrl}}/v1/{{traceId}}");
  expect(await exchange.send(draft)).toBe("warned");
  // The tab closed.
  shell.tabs = [];
  pruneDrafts();
  expect(await exchange.send(draft)).toBe("ignored");
  expect(ipc.sends).toHaveLength(3);
});

test("the_sending_state_knows_when_the_request_left", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  // Dea may still be asking whether to send.
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "sending",
    started: false,
  });
  ipc.emitToSends({
    type: "started",
    method: "GET",
    url: "https://api.harbor.test/v1/shipments",
  });
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "sending",
    started: true,
    host: "api.harbor.test",
  });
  ipc.finishSends(ok200);
  await sent;
});

test("a_draft_changed_since_its_inspection_is_inspected_again", async () => {
  const ipc = fakeRequestsIpc({
    files,
    inspection: (_path, request) => ({
      undefined: request.url.includes("{{carrierCode}}") ? ["carrierCode"] : [],
    }),
  });
  const draft = await openDraft("t1", PATH);
  await draft.inspect();
  // Inspected as it is: no second look.
  await exchangeFor("t1").send(draft);
  expect(calls(ipc, "inspect_request")).toHaveLength(1);
  expect(ipc.sends).toHaveLength(1);
  // Edited since: the warning is about the edit.
  draft.typeUrl("{{baseUrl}}/v1/{{carrierCode}}");
  await exchangeFor("t1").send(draft);
  expect(calls(ipc, "inspect_request")).toHaveLength(2);
  expect(exchangeFor("t1").warning).toEqual(["carrierCode"]);
  expect(ipc.sends).toHaveLength(1);
});

/** Warns of `{{carrierCode}}` in the URL, after `delay` ms. */
const carrierCheck =
  (delay: number): FakeInspection =>
  async (_path, request) => {
    await new Promise((done) => setTimeout(done, delay));
    return {
      undefined: request.url.includes("{{carrierCode}}") ? ["carrierCode"] : [],
    };
  };

test("a_send_right_after_typing_checks_the_typed_url", async () => {
  const ipc = fakeRequestsIpc({ files, inspection: carrierCheck(0) });
  const draft = await openDraft("t1", PATH);
  await draft.inspect();
  // Slower than what is left of the tab's own look, which comes mid-check.
  ipc.inspection = carrierCheck(INSPECT_DELAY + 100);
  draft.typeUrl("{{baseUrl}}/v1/{{carrierCode}}");
  draft.scheduleInspection(); // as the tab does after each edit
  await exchangeFor("t1").send(draft);
  expect(exchangeFor("t1").warning).toEqual(["carrierCode"]);
  expect(ipc.sends).toEqual([]);
});

test("typing_during_the_check_is_checked_too", async () => {
  const ipc = fakeRequestsIpc({ files, inspection: carrierCheck(0) });
  const draft = await openDraft("t1", PATH);
  await draft.inspect();
  ipc.inspection = carrierCheck(50);
  draft.typeUrl("{{baseUrl}}/v1/other");
  const sent = exchangeFor("t1").send(draft);
  expect(calls(ipc, "inspect_request")).toHaveLength(2);
  draft.typeUrl("{{baseUrl}}/v1/{{carrierCode}}");
  await sent;
  expect(exchangeFor("t1").warning).toEqual(["carrierCode"]);
  expect(ipc.sends).toEqual([]);
});

test("a_draft_that_keeps_changing_is_checked_a_few_times_at_most", async () => {
  const ipc = fakeRequestsIpc({ files });
  const draft = await openDraft("t1", PATH);
  let typed = 0;
  // Each look sees a draft that has moved on again.
  ipc.inspection = () => {
    draft.typeUrl(`{{baseUrl}}/v1/${++typed}`);
    return {};
  };
  await exchangeFor("t1").send(draft);
  expect(calls(ipc, "inspect_request")).toHaveLength(3);
  expect(ipc.sends).toHaveLength(1);
});

test("closing_the_tab_while_checking_sends_nothing", async () => {
  const ipc = fakeRequestsIpc({ files, inspection: carrierCheck(50) });
  const draft = await openDraft("t1", PATH);
  const sent = exchangeFor("t1").send(draft);
  shell.tabs = [];
  pruneDrafts();
  await sent;
  expect(ipc.sends).toEqual([]);
  expect(calls(ipc, "cancel_send")).toEqual([]);
});

test("not_confirmed_brings_back_the_previous_response", async () => {
  const ipc = fakeRequestsIpc({ files });
  const draft = await openDraft("t1", PATH);
  await exchangeFor("t1").send(draft);
  const before = exchangeFor("t1").state;
  expect(before.phase).toBe("done");
  const version = cookiesVersion();
  ipc.send.outcome = { type: "notConfirmed" };
  await exchangeFor("t1").send(draft);
  expect(exchangeFor("t1").state).toEqual(before);
  expect(cookiesVersion()).toBe(version);
  expect(calls(ipc, "release_response")).toEqual([]);
});

test("cancel_ends_a_running_send", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  exchangeFor("t1").cancel();
  await sent;
  expect(calls(ipc, "cancel_send")).toEqual([{ id: "t1" }]);
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "cancelled",
    head: null,
    events: [],
  });
});

test("closing_a_sending_tab_cancels_it", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const draft = await openDraft("t1", PATH);
  void startSend(draft);
  await vi.waitFor(() => expect(ipc.sends).toHaveLength(1));
  shell.tabs = [];
  pruneDrafts();
  const ends = ipc.calls
    .map(([cmd, args]) => [cmd, args.id])
    .filter(([cmd]) => cmd === "cancel_send" || cmd === "release_response");
  expect(ends).toEqual([
    ["cancel_send", "t1"],
    ["release_response", "t1"],
  ]);
  expect(exchanges.has("t1")).toBe(false);
});

test("closing_a_background_tab_that_is_sending_cancels_it", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const draft = await openDraft("t1", PATH);
  void startSend(draft);
  await vi.waitFor(() => expect(ipc.sends).toHaveLength(1));
  // The tab is not shown, so nothing unmounts: only the tab list changes.
  await shell.closeTab("t1");
  await vi.waitFor(() => expect(exchanges.has("t1")).toBe(false));
  const ends = ipc.calls
    .map(([cmd, args]) => [cmd, args.id])
    .filter(([cmd]) => cmd === "cancel_send" || cmd === "release_response");
  expect(ends).toEqual([
    ["cancel_send", "t1"],
    ["release_response", "t1"],
  ]);
});

test("a_response_keeps_the_environment_it_was_sent_in", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  await project.setEnvironment("staging");
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  await project.setEnvironment("local");
  ipc.finishSends(ok200);
  await sent;
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "done",
    environment: "staging",
  });
});

test("a_failure_keeps_the_environment_too", async () => {
  fakeRequestsIpc({
    files,
    send: {
      outcome: {
        type: "failed",
        failure: {
          kind: "dns",
          title: "Host not found",
          message: "api.harbor.test could not be resolved.",
          code: "ENOTFOUND",
        },
      },
    },
  });
  await project.setEnvironment("staging");
  const draft = await openDraft("t1", PATH);
  await exchangeFor("t1").send(draft);
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "failed",
    environment: "staging",
    failure: { code: "ENOTFOUND" },
  });
});

test("events_build_the_sending_state", async () => {
  const events: SendEvent[] = [
    {
      type: "started",
      method: "GET",
      url: "https://api.harbor.test:8443/v1/shipments",
    },
    { type: "phase", phase: "dns", ms: 4 },
    { type: "phase", phase: "connect", ms: 12 },
    { type: "phase", phase: "tls", ms: 18 },
    {
      type: "head",
      status: 200,
      reason: "OK",
      headers: [{ name: "content-type", value: "application/json" }],
    },
    { type: "body", received: 6, text: '[{"id"' },
    { type: "body", received: 11, text: ":1}]" },
    { type: "body", received: 2048, text: null },
  ];
  const ipc = fakeRequestsIpc({ files, send: { hold: true, events } });
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "sending",
    host: "api.harbor.test:8443",
    steps: [
      { kind: "dns", ms: 4 },
      { kind: "connect", ms: 12 },
      { kind: "tls", ms: 18 },
    ],
    head: {
      status: 200,
      reason: "OK",
      headers: [{ name: "content-type", value: "application/json" }],
    },
    received: 2048,
    text: '[{"id":1}]',
    events: [],
  });
  // The size shown at the end is the response's, not the last count.
  ipc.finishSends({
    type: "response",
    response: responseInfo({ size: 4096 }),
  });
  await sent;
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "done",
    response: { size: 4096 },
  });
});

test("a_reused_connection_and_server_events_are_listed", async () => {
  const events: SendEvent[] = [
    { type: "started", method: "GET", url: "not a url" },
    { type: "reused" },
    {
      type: "event",
      atMs: 120,
      event: "status",
      data: '{"state":"loaded"}',
      id: "1",
    },
    { type: "event", atMs: 240, event: null, data: "ping", id: null },
  ];
  const ipc = fakeRequestsIpc({ files, send: { hold: true, events } });
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  const rows = [
    { atMs: 120, event: "status", data: '{"state":"loaded"}', id: "1" },
    { atMs: 240, event: null, data: "ping", id: null },
  ];
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "sending",
    host: null,
    steps: [{ kind: "reused", ms: 0 }],
    events: rows,
  });
  ipc.finishSends(ok200);
  await sent;
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "done",
    events: rows,
  });
});

test("a_cancelled_stream_keeps_its_head_and_events", async () => {
  const events: SendEvent[] = [
    { type: "head", status: 200, reason: "OK", headers: [] },
    { type: "event", atMs: 5, event: null, data: "one", id: null },
  ];
  fakeRequestsIpc({ files, send: { hold: true, events } });
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  exchangeFor("t1").cancel();
  await sent;
  const state = exchangeFor("t1").state;
  expect(state).toMatchObject({
    phase: "cancelled",
    head: { status: 200, reason: "OK" },
    events: [{ data: "one" }],
  });
  expect(state.phase === "cancelled" && state.afterMs).toBeGreaterThanOrEqual(
    0,
  );
});

test("cookies_not_saved_are_reported", async () => {
  fakeRequestsIpc({
    files,
    send: {
      outcome: {
        type: "response",
        response: responseInfo({ cookiesNotSaved: "the disk is full" }),
      },
    },
  });
  const draft = await openDraft("t1", PATH);
  await exchangeFor("t1").send(draft);
  expect(shell.toasts).toEqual([
    "Cookies from this response were not saved: the disk is full",
  ]);
});

test("every_finished_send_moves_the_cookies_version", async () => {
  const ipc = fakeRequestsIpc({ files });
  const draft = await openDraft("t1", PATH);
  const start = cookiesVersion();
  await exchangeFor("t1").send(draft);
  expect(cookiesVersion()).toBe(start + 1);
  ipc.send.outcome = {
    type: "failed",
    failure: { kind: "refused", title: "Refused", message: "", code: null },
  };
  await exchangeFor("t1").send(draft);
  ipc.send.outcome = { type: "cancelled" };
  await exchangeFor("t1").send(draft);
  expect(cookiesVersion()).toBe(start + 3);
});

test("a_send_that_ends_without_a_response_lets_the_old_one_go", async () => {
  const ipc = fakeRequestsIpc({ files });
  const draft = await openDraft("t1", PATH);
  await exchangeFor("t1").send(draft);
  expect(calls(ipc, "release_response")).toEqual([]);
  ipc.send.outcome = { type: "cancelled" };
  await exchangeFor("t1").send(draft);
  expect(calls(ipc, "release_response")).toEqual([{ id: "t1" }]);
});

test("a_window_without_a_project_sends_nothing_and_says_why", async () => {
  const ipc = fakeRequestsIpc({ files });
  const draft = await openDraft("t1", PATH);
  ipc.failures.set("send_request", {
    kind: "noProject",
    message: "this window has no project",
  });
  await exchangeFor("t1").send(draft);
  expect(exchangeFor("t1").state.phase).toBe("idle");
  expect(shell.toasts).toEqual(["This window has no project"]);
});

test("an_endless_stream_keeps_only_the_last_events", async () => {
  const events: SendEvent[] = Array.from({ length: 10_005 }, (_, n) => ({
    type: "event",
    atMs: n,
    event: null,
    data: `d${n}`,
    id: null,
  }));
  const ipc = fakeRequestsIpc({ files, send: { hold: true, events } });
  const draft = await openDraft("t1", PATH);
  await startSend(draft);
  const sending = exchangeFor("t1").state;
  if (sending.phase !== "sending") throw new Error("not sending");
  expect(sending.events).toHaveLength(10_000);
  expect(sending.events[0]?.data).toBe("d5");
  expect(sending.droppedEvents).toBe(5);
  ipc.finishSends();
  await vi.waitFor(() => expect(exchangeFor("t1").state.phase).toBe("done"));
  expect(exchangeFor("t1").state).toMatchObject({ droppedEvents: 5 });
});

/** The channel the `n`th send (from 0) hears its progress on. */
function channelOf(ipc: RequestsIpc, n: number): Channel<SendEvent> {
  return calls(ipc, "send_request")[n]?.events as Channel<SendEvent>;
}

const late = (data: string): SendEvent => ({
  type: "event",
  atMs: 9,
  event: null,
  data,
  id: null,
});

test("events_after_the_outcome_join_the_finished_send", async () => {
  // Large messages come another way than small ones, and can land after
  // the outcome.
  const events: SendEvent[] = Array.from({ length: MAX_EVENTS }, (_, n) => ({
    type: "event",
    atMs: n,
    event: null,
    data: `d${n}`,
    id: null,
  }));
  const ipc = fakeRequestsIpc({ files, send: { hold: true, events } });
  const draft = await openDraft("t1", PATH);
  const { sent } = await startSend(draft);
  ipc.finishSends(ok200);
  await sent;
  channelOf(ipc, 0).onmessage(late("after done"));
  let state = exchangeFor("t1").state;
  if (state.phase !== "done") throw new Error("not done");
  // The cap still holds.
  expect(state.events).toHaveLength(MAX_EVENTS);
  expect(state.events.at(-1)?.data).toBe("after done");
  expect(state.droppedEvents).toBe(1);

  ipc.send.events = [];
  const again = exchangeFor("t1").send(draft);
  await vi.waitFor(() => expect(ipc.sends).toHaveLength(2));
  exchangeFor("t1").cancel();
  await again;
  channelOf(ipc, 1).onmessage(late("after cancel"));
  state = exchangeFor("t1").state;
  expect(state).toMatchObject({
    phase: "cancelled",
    events: [{ data: "after cancel" }],
  });
});

test("events_of_an_older_send_never_reach_a_newer_one", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const draft = await openDraft("t1", PATH);
  const first = await startSend(draft);
  ipc.finishSends(ok200);
  await first.sent;

  const second = exchangeFor("t1").send(draft);
  await vi.waitFor(() => expect(ipc.sends).toHaveLength(2));
  channelOf(ipc, 0).onmessage(late("old while sending"));
  channelOf(ipc, 0).onmessage({ type: "body", received: 3, text: "old" });
  expect(exchangeFor("t1").state).toMatchObject({
    phase: "sending",
    events: [],
    text: "",
    received: 0,
  });
  ipc.finishSends(ok200);
  await second;
  channelOf(ipc, 0).onmessage(late("old after done"));
  expect(exchangeFor("t1").state).toMatchObject({ phase: "done", events: [] });
});

test("text_bytes_count_utf8_not_characters", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: {
      hold: true,
      events: [
        { type: "body", received: 6, text: "éé" },
        { type: "body", received: 9, text: "€" },
      ],
    },
  });
  const draft = await openDraft("t1", PATH);
  await startSend(draft);
  expect(exchangeFor("t1").state).toMatchObject({
    text: "éé€",
    textBytes: 7,
  });
  ipc.finishSends();
});

test("a_second_send_sends_while_the_warning_shows", async () => {
  vi.useFakeTimers({ toFake: ["performance"] });
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["traceId"] },
  });
  const draft = await openDraft("t1", PATH);
  const exchange = exchangeFor("t1");
  expect(await exchange.send(draft)).toBe("warned");
  vi.advanceTimersByTime(SECOND_SEND_GAP_MS);
  expect(await exchange.send(draft)).toBe("sent");
  expect(ipc.sends).toHaveLength(1);
  expect(exchange.warning).toBeNull();
});

test("a_second_send_right_after_the_warning_is_ignored", async () => {
  vi.useFakeTimers({ toFake: ["performance"] });
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["traceId"] },
  });
  const draft = await openDraft("t1", PATH);
  const exchange = exchangeFor("t1");
  expect(await exchange.send(draft)).toBe("warned");
  vi.advanceTimersByTime(SECOND_SEND_GAP_MS - 1);
  expect(await exchange.send(draft)).toBe("ignored");
  expect(ipc.sends).toHaveLength(0);
  expect(exchange.warning).toEqual(["traceId"]);
});

test("different_names_warn_again", async () => {
  vi.useFakeTimers({ toFake: ["performance"] });
  const ipc = fakeRequestsIpc({
    files,
    inspection: (_path, request) => ({
      undefined: request.url.includes("{{tenant}}") ? ["tenant"] : ["traceId"],
    }),
  });
  const draft = await openDraft("t1", PATH);
  const exchange = exchangeFor("t1");
  expect(await exchange.send(draft)).toBe("warned");
  expect(exchange.warning).toEqual(["traceId"]);
  vi.advanceTimersByTime(SECOND_SEND_GAP_MS * 2);
  draft.typeUrl("{{baseUrl}}/v1/{{tenant}}");
  expect(await exchange.send(draft)).toBe("warned");
  expect(exchange.warning).toEqual(["tenant"]);
  // The count started again with the new banner.
  vi.advanceTimersByTime(SECOND_SEND_GAP_MS - 1);
  expect(await exchange.send(draft)).toBe("ignored");
  vi.advanceTimersByTime(1);
  expect(await exchange.send(draft)).toBe("sent");
  expect(ipc.sends).toHaveLength(1);
});

test("clearing_the_warning_makes_the_next_send_warn_again", async () => {
  vi.useFakeTimers({ toFake: ["performance"] });
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["traceId"] },
  });
  const draft = await openDraft("t1", PATH);
  const exchange = exchangeFor("t1");
  await exchange.send(draft);
  vi.advanceTimersByTime(SECOND_SEND_GAP_MS);
  exchange.clearWarning();
  expect(exchange.warning).toBeNull();
  expect(await exchange.send(draft)).toBe("warned");
  expect(ipc.sends).toHaveLength(0);
});
