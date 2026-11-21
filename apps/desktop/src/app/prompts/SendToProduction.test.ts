import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { flushSync } from "svelte";
import { expect, test } from "vitest";
import type { SendConfirmEvent } from "$lib/ipc/send";
import {
  emitSendConfirm,
  emitSendConfirmWithdrawn,
  fakeSendConfirm,
} from "./send-confirm.test-support";
import { SendConfirmations, sendConfirmations } from "./send-confirm.svelte";
import SendToProduction from "./SendToProduction.svelte";

const post: SendConfirmEvent = {
  id: 1,
  method: "POST",
  url: "https://api.test/v1/x",
  host: "api.test",
  environment: "production",
  undefined: [],
  canSkip: false,
  secrets: true,
};

test("focus_starts_on_cancel_and_enter_does_not_send", async () => {
  const answers = fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  const cancel = await screen.findByRole("button", { name: /Cancel/ });
  expect(cancel).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(answers).toEqual([[1, "cancel"]]);
});

test("escape_and_send_answer", async () => {
  const answers = fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  await userEvent.click(
    await screen.findByRole("button", { name: "Send to production" }),
  );
  emitSendConfirm({ ...post, id: 2 });
  await screen.findByRole("dialog");
  await userEvent.keyboard("{Escape}");
  expect(answers).toEqual([
    [1, "send"],
    [2, "cancel"],
  ]);
});

test("dont_ask_again_is_offered_only_when_it_can_skip", async () => {
  fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  await screen.findByRole("dialog");
  expect(screen.queryByLabelText(/Don't ask again/)).toBeNull();
  emitSendConfirm({ ...post, id: 2, method: "GET", canSkip: true });
  await userEvent.click(screen.getByRole("button", { name: /Cancel/ }));
  const box = await screen.findByLabelText(/Don't ask again in this session/);
  expect(box).not.toBeChecked();
  expect(screen.getByRole("dialog")).toHaveTextContent(
    "Offered for GET, HEAD and OPTIONS only",
  );
});

test("dont_ask_again_answers_send_and_stop_asking", async () => {
  const answers = fakeSendConfirm({ ...post, method: "GET", canSkip: true });
  render(SendToProduction, { confirmations: sendConfirmations });
  await userEvent.click(
    await screen.findByLabelText(/Don't ask again in this session/),
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Send to production" }),
  );
  expect(answers).toEqual([[1, "sendAndStopAsking"]]);
});

test("the_url_and_environment_are_named", async () => {
  fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  const dialog = await screen.findByRole("dialog", {
    name: "Send this request to production?",
  });
  expect(dialog).toHaveTextContent("POST");
  expect(dialog).toHaveTextContent("https://api.test/v1/x");
  expect(within(dialog).getByText("production")).toHaveClass("env-prod");
});

test("the_subtitle_mentions_secrets_only_when_used", async () => {
  fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  const dialog = await screen.findByRole("dialog");
  expect(dialog).toHaveTextContent(
    "It uses production secrets and reaches real data",
  );
  emitSendConfirm({ ...post, id: 2, secrets: false });
  await userEvent.click(screen.getByRole("button", { name: /Cancel/ }));
  const next = await screen.findByRole("dialog");
  expect(next).toHaveTextContent("It reaches real data");
  expect(next).not.toHaveTextContent("secrets");
});

test("queued_confirmations_show_one_at_a_time", async () => {
  const answers = fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  emitSendConfirm({ ...post, id: 2, url: "https://api.test/second" });
  emitSendConfirm({ ...post, id: 3, url: "https://api.test/third" });
  flushSync();
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(screen.getByRole("dialog")).toHaveTextContent("/v1/x");
  await userEvent.click(screen.getByRole("button", { name: /Cancel/ }));
  expect(screen.getByRole("dialog")).toHaveTextContent("/second");
  // A withdrawn one never shows; an unknown id is ignored.
  emitSendConfirmWithdrawn(3);
  emitSendConfirmWithdrawn(99);
  await userEvent.click(screen.getByRole("button", { name: /Cancel/ }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(answers).toEqual([
    [1, "cancel"],
    [2, "cancel"],
  ]);
});

test("a_withdrawn_confirmation_leaves_the_queue", async () => {
  const answers = fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  emitSendConfirm({ ...post, id: 2, url: "https://api.test/second" });
  await screen.findByRole("dialog");
  // The shown one is withdrawn: its dialog goes, the next one comes up.
  emitSendConfirmWithdrawn(1);
  expect(await screen.findByText("https://api.test/second")).toBeVisible();
  emitSendConfirmWithdrawn(2);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(answers).toEqual([]);
});

test("an_unknown_withdrawn_id_leaves_the_current_one_showing", async () => {
  fakeSendConfirm(post);
  render(SendToProduction, { confirmations: sendConfirmations });
  await screen.findByRole("dialog");
  emitSendConfirmWithdrawn(99);
  flushSync();
  expect(screen.getByRole("dialog")).toHaveTextContent("/v1/x");
});

test("focus_stays_on_cancel_when_the_checkbox_is_offered", async () => {
  const answers = fakeSendConfirm({ ...post, method: "GET", canSkip: true });
  render(SendToProduction, { confirmations: sendConfirmations });
  expect(await screen.findByRole("button", { name: /Cancel/ })).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(answers).toEqual([[1, "cancel"]]);
});

test("dont_ask_again_starts_unchecked_for_each_confirmation", async () => {
  const answers = fakeSendConfirm({ ...post, method: "GET", canSkip: true });
  render(SendToProduction, { confirmations: sendConfirmations });
  emitSendConfirm({ ...post, id: 2, method: "GET", canSkip: true });
  await userEvent.click(
    await screen.findByLabelText(/Don't ask again in this session/),
  );
  await userEvent.click(screen.getByRole("button", { name: /Cancel/ }));
  expect(
    await screen.findByLabelText(/Don't ask again in this session/),
  ).not.toBeChecked();
  await userEvent.click(
    screen.getByRole("button", { name: "Send to production" }),
  );
  expect(answers).toEqual([
    [1, "cancel"],
    [2, "send"],
  ]);
});

test("a_failed_listener_is_reported_and_cleaned_up", async () => {
  const confirmations = new SendConfirmations();
  let stopped = 0;
  await expect(
    confirmations.listen(
      () =>
        Promise.resolve(() => {
          stopped += 1;
        }),
      () => Promise.resolve(),
      () => Promise.reject(new Error("no window")),
    ),
  ).rejects.toThrow("no window");
  expect(stopped).toBe(1);
});
