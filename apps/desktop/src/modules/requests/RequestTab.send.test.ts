import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import type { SendEvent } from "$lib/ipc/bindings/SendEvent";
import UndefinedWarning from "./components/UndefinedWarning.svelte";
import type { FakeProject } from "./fake-project.svelte";
import { manifest } from "./index";
import sendActiveTab from "./intents/send";
import {
  openRequestTab,
  setUpRequestTabs,
  useProduction,
  type FakeShell,
} from "./request-tab-support.svelte";
import {
  fakeRequestsIpc,
  responseInfo,
  type RequestsIpc,
} from "./test-support";

const PATH = "requests/shipments/list.yaml";
const file: RequestFile = {
  name: "List shipments",
  method: "GET",
  url: "{{baseUrl}}/v1/shipments",
};
const files = { [PATH]: file };
/** What Dea tells just before the request leaves. */
const started: SendEvent = {
  type: "started",
  method: "GET",
  url: "https://api.harbor.test/v1/shipments",
};

let shell: FakeShell;
let project: FakeProject;

beforeEach(() => {
  ({ shell, project } = setUpRequestTabs());
});

const calls = (ipc: RequestsIpc, cmd: string) =>
  ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);

const sendButton = () => screen.findByRole("button", { name: "Send" });

/** The request bar: the method, the URL, Send and Save. */
const requestBar = () =>
  screen.getByRole("button", { name: "Save request" }).parentElement!;

/** The keycaps shown in `button`, and whether its name leaves them out. */
const keycaps = (button: HTMLElement) =>
  [...button.querySelectorAll("kbd")].map((k) => [
    k.textContent,
    k.closest('[aria-hidden="true"]') !== null,
  ]);

test("the_send_button_turns_into_cancel_while_sending", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: { hold: true, events: [started] },
  });
  const { id } = openRequestTab(PATH);
  const send = await sendButton();
  expect(keycaps(send)).toEqual([
    ["Ctrl", true],
    ["Enter", true],
  ]);
  await userEvent.click(send);
  const cancel = await screen.findByRole("button", { name: "Cancel" });
  expect(keycaps(cancel)).toEqual([["Esc", true]]);
  // The same button, so the focus stays where it was.
  expect(cancel).toBe(send);
  await userEvent.click(cancel);
  expect(calls(ipc, "cancel_send")).toEqual([{ id }]);
  expect(await sendButton()).toBeInTheDocument();
});

test("the_send_button_keeps_room_for_its_other_label", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: { hold: true, events: [started] },
  });
  openRequestTab(PATH);
  const button = await sendButton();
  // The label not in use stays laid out, hidden, so the width holds.
  const hidden = () =>
    button
      .querySelector('.face[aria-hidden="true"]')
      ?.firstChild?.textContent?.trim();
  expect(hidden()).toBe("Cancel");
  await userEvent.click(button);
  await screen.findByRole("button", { name: "Cancel" });
  expect(hidden()).toBe("Send");
  ipc.finishSends();
});

test("escape_cancels_a_running_send", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: { hold: true, events: [started] },
  });
  const { id } = openRequestTab(PATH);
  await userEvent.click(await sendButton());
  await screen.findByRole("button", { name: "Cancel" });
  await userEvent.click(screen.getByRole("textbox", { name: "URL" }));
  await userEvent.keyboard("{Escape}");
  expect(calls(ipc, "cancel_send")).toEqual([{ id }]);
  expect(await sendButton()).toBeInTheDocument();
});

test("escape_with_nothing_focused_cancels_too", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: { hold: true, events: [started] },
  });
  const { id } = openRequestTab(PATH);
  await userEvent.click(await sendButton());
  await screen.findByRole("button", { name: "Cancel" });
  (document.activeElement as HTMLElement | null)?.blur();
  await userEvent.keyboard("{Escape}");
  expect(calls(ipc, "cancel_send")).toEqual([{ id }]);
});

test("an_escape_taken_by_the_field_or_with_nothing_running_cancels_nothing", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: { hold: true, events: [started] },
  });
  openRequestTab(PATH);
  const url = await screen.findByRole("textbox", { name: "URL" });
  await userEvent.click(url);
  await userEvent.keyboard("{Escape}");
  await userEvent.click(await sendButton());
  await screen.findByRole("button", { name: "Cancel" });
  url.addEventListener("keydown", (e) => e.preventDefault());
  await userEvent.click(url);
  await userEvent.keyboard("{Escape}");
  expect(calls(ipc, "cancel_send")).toEqual([]);
  ipc.finishSends();
});

test("send_follows_the_window_environment", async () => {
  fakeRequestsIpc({ files });
  await project.setEnvironment("staging");
  openRequestTab(PATH);
  expect(await screen.findByRole("button", { name: /^Send/ })).not.toHaveClass(
    "danger-solid",
  );
  await useProduction(project);
  const send = await screen.findByRole("button", {
    name: /^Send to production/,
  });
  expect(send).toHaveClass("danger-solid");
  expect(within(requestBar()).getByText("production")).toHaveClass("env-prod");
  await project.setEnvironment("staging");
  await waitFor(() =>
    expect(within(requestBar()).queryByText("production")).toBeNull(),
  );
  expect(screen.getByRole("button", { name: /^Send/ })).not.toHaveClass(
    "danger-solid",
  );
});

test("the_bar_reads_cancel_only_after_the_request_started", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  await useProduction(project);
  openRequestTab(PATH);
  const send = await screen.findByRole("button", {
    name: "Send to production",
  });
  await userEvent.click(send);
  await waitFor(() => expect(ipc.sends).toHaveLength(1));
  shell.confirmingSend = true;
  // Asked, not sent: the button still reads as before, and a second press
  // sends nothing more.
  await userEvent.click(send);
  expect(send).toHaveAccessibleName("Send to production");
  expect(send).toHaveClass("danger-solid");
  expect(ipc.sends).toHaveLength(1);
  shell.confirmingSend = false;
  ipc.emitToSends(started);
  await waitFor(() => expect(send).toHaveAccessibleName("Cancel"));
  expect(send).not.toHaveClass("danger-solid");
  ipc.finishSends();
});

test("escape_cancels_a_send_waiting_to_leave", async () => {
  const ipc = fakeRequestsIpc({ files, send: { hold: true } });
  const { id } = openRequestTab(PATH);
  await userEvent.click(await sendButton());
  await waitFor(() => expect(ipc.sends).toHaveLength(1));
  await userEvent.click(screen.getByRole("textbox", { name: "URL" }));
  await userEvent.keyboard("{Escape}");
  expect(calls(ipc, "cancel_send")).toEqual([{ id }]);
});

/**
 * Sends the active tab with `Mod+Enter` while `ipc` holds the send, as Dea
 * does while it asks: the dialog takes the focus, and is answered Cancel.
 */
async function declineFromKeyboard(ipc: RequestsIpc, id: string) {
  ipc.send = { ...ipc.send, hold: true };
  shell.activateTab(id);
  const sent = sendActiveTab();
  await waitFor(() => expect(ipc.sends).toHaveLength(2));
  shell.confirmingSend = true;
  const dialog = document.body.appendChild(document.createElement("button"));
  dialog.focus();
  dialog.remove();
  shell.confirmingSend = false;
  ipc.finishSends({ type: "notConfirmed" });
  await sent;
}

test("cancelling_the_dialog_returns_focus", async () => {
  const ipc = fakeRequestsIpc({
    files,
    send: {
      outcome: { type: "response", response: responseInfo({ status: 201 }) },
    },
  });
  const { id } = openRequestTab(PATH);
  await userEvent.click(await sendButton());
  await screen.findByText("201");
  const url = screen.getByRole("textbox", { name: "URL" });
  await userEvent.click(url);
  await declineFromKeyboard(ipc, id);
  await waitFor(() => expect(url).toHaveFocus());
  const bar = screen.getByRole("toolbar", { name: "Response" });
  expect(within(bar).getByText("201")).toBeInTheDocument();
});

test("a_declined_send_from_a_control_that_went_focuses_send", async () => {
  const ipc = fakeRequestsIpc({ files });
  const { id } = openRequestTab(PATH);
  await userEvent.click(await sendButton());
  // The response's own control goes while the send waits.
  const copy = await screen.findByRole("button", { name: "Copy response" });
  copy.focus();
  await declineFromKeyboard(ipc, id);
  expect(copy.isConnected).toBe(false);
  await waitFor(() =>
    expect(document.querySelector("button[data-send]")).toHaveFocus(),
  );
});

test("mod_enter_sends_the_active_request_tab", async () => {
  expect(manifest.shortcuts).toContainEqual({
    command: "requests.send",
    chord: "Mod+Enter",
  });
  expect(manifest.commands).toContainEqual({
    id: "requests.send",
    title: "Send request",
    action: { type: "intent", intent: "requests.send" },
    scope: "project",
  });
  expect(manifest.intents?.map((i) => i.intent)).toContain("requests.send");

  const ipc = fakeRequestsIpc({ files });
  openRequestTab("requests/shipments/other.yaml");
  const { id } = openRequestTab(PATH);
  await sendButton();
  shell.activateTab(id);
  await sendActiveTab();
  expect(ipc.sends).toMatchObject([{ id, path: PATH }]);
});

test("mod_enter_without_a_request_tab_says_so", async () => {
  const ipc = fakeRequestsIpc({ files });
  await sendActiveTab();
  const folder = shell.openTab("requests.folder", "Shipments", {
    path: "requests/shipments",
  });
  shell.activateTab(folder);
  await sendActiveTab();
  expect(shell.toasts).toEqual([
    "Open a request to send it.",
    "Open a request to send it.",
  ]);
  expect(ipc.sends).toEqual([]);
});

test("undefined_variables_show_a_warning_with_define_and_send_anyway", async () => {
  const ipc = fakeRequestsIpc({
    files,
    inspection: { undefined: ["carrierCode"] },
  });
  openRequestTab(PATH);
  await userEvent.click(await sendButton());
  const banner = await screen.findByRole("status");
  expect(banner).toHaveTextContent(
    "carrierCode is not defined in this collection. It is sent as written.",
  );
  expect(ipc.sends).toEqual([]);

  await userEvent.click(
    within(banner).getByRole("button", { name: "Define it" }),
  );
  expect(shell.commands).toEqual(["project.editEnvironments"]);

  await userEvent.click(
    within(banner).getByRole("button", { name: "Send anyway" }),
  );
  await waitFor(() => expect(ipc.sends).toHaveLength(1));
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
});

test("the_warning_names_the_environment_in_use", async () => {
  fakeRequestsIpc({
    files,
    inspection: { undefined: ["carrierCode", "region"] },
  });
  await project.setEnvironment("staging");
  openRequestTab(PATH);
  await userEvent.click(await sendButton());
  expect(await screen.findByRole("status")).toHaveTextContent(
    "carrierCode and region are not defined in staging or in this collection. They are sent as written.",
  );
});

test.each([
  {
    names: ["carrierCode"],
    environment: "staging",
    text: "carrierCode is not defined in staging or in this collection. It is sent as written.",
  },
  {
    names: ["carrierCode", "region", "token"],
    environment: "staging",
    text: "carrierCode, region and token are not defined in staging or in this collection. They are sent as written.",
  },
  {
    names: ["carrierCode"],
    environment: null,
    text: "carrierCode is not defined in this collection. It is sent as written.",
  },
  {
    names: ["carrierCode", "region"],
    environment: null,
    text: "carrierCode and region are not defined in this collection. They are sent as written.",
  },
])("warning_text: $text", ({ names, environment, text }) => {
  const ondefine = vi.fn();
  const onsendanyway = vi.fn();
  render(UndefinedWarning, { names, environment, ondefine, onsendanyway });
  const banner = screen.getByRole("status");
  expect(banner).toHaveTextContent(text);
  expect(
    [...banner.querySelectorAll("code")].map((c) => c.textContent),
  ).toEqual(names);
});
