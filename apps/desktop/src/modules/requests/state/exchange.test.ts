import { beforeEach, expect, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { SendEvent } from "$lib/ipc/bindings/SendEvent";
import type { SendOutcome } from "$lib/ipc/bindings/SendOutcome";
import type { FakeProject } from "../fake-project.svelte";
import {
  setUpRequestTabs,
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
import { cookiesVersion, exchangeFor, exchanges } from "./exchange.svelte";

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
): Promise<{ sent: Promise<void> }> {
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
