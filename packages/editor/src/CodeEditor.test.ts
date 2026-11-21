import { render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import { expect, test, vi } from "vitest";
import type { EditorView } from "@codemirror/view";
import CodeEditor from "./CodeEditor.svelte";
import type { VariableState } from "./variables";

const none: VariableState = { defined: () => false, describe: () => null };

function mount(props: Record<string, unknown> = {}) {
  const result = render(CodeEditor, {
    value: '{ "a": {{missing}} }',
    language: "json",
    label: "Request body",
    variables: none,
    ...props,
  });
  return {
    ...result,
    view: () =>
      (result.component as unknown as { getView(): EditorView }).getView(),
  };
}

test("marks undefined variables", () => {
  const { container } = mount();
  expect(container.querySelector(".cm-var-undefined")).not.toBeNull();
});

test("an undefined variable is underlined, not only coloured", () => {
  const { container } = mount();
  const mark = container.querySelector(".cm-var-undefined") as HTMLElement;
  expect(getComputedStyle(mark).textDecorationLine).toBe("underline");
  expect(getComputedStyle(mark).textDecorationStyle).toBe("wavy");
});

test("marks defined variables", () => {
  const { container } = mount({
    variables: { defined: () => true, describe: () => null },
  });
  expect(container.querySelector(".cm-var")).not.toBeNull();
  expect(container.querySelector(".cm-var-undefined")).toBeNull();
});

test("labels the content area", () => {
  mount();
  expect(
    screen.getByRole("textbox", { name: "Request body" }),
  ).toBeInTheDocument();
});

test("reports edits through onchange", () => {
  const onchange = vi.fn();
  const { view } = mount({ onchange });
  view().dispatch({ changes: { from: 0, insert: " " } });
  expect(onchange).toHaveBeenCalledWith(' { "a": {{missing}} }');
});

test("shows the lint error under the editor", async () => {
  const { container } = mount({ value: '{\n  "a": \n}' });
  await tick();
  const status = container.querySelector("p[aria-live='polite']");
  expect(status?.textContent).toMatch(/^Line 3: /);
});

test("shows no status for valid JSON", () => {
  const { container } = mount({ value: '{ "a": {{ok}} }' });
  expect(container.querySelector("p[aria-live='polite']")?.textContent).toBe(
    "",
  );
});

test("formats JSON with Mod+Shift+F", () => {
  const onchange = vi.fn();
  const { view } = mount({ value: '{"a":1}', onchange });
  view().contentDOM.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "F",
      code: "KeyF",
      keyCode: 70,
      shiftKey: true,
      ctrlKey: true,
      bubbles: true,
    }),
  );
  expect(view().state.doc.toString()).toBe('{\n  "a": 1\n}');
});

test("follows an external value change", async () => {
  const { view, rerender } = mount({ value: "one", language: "text" });
  await rerender({ value: "two" });
  expect(view().state.doc.toString()).toBe("two");
});

test("updates variable colours when the state changes", async () => {
  const { container, rerender } = mount();
  await rerender({ variables: { defined: () => true, describe: () => null } });
  expect(container.querySelector(".cm-var-undefined")).toBeNull();
  expect(container.querySelector(".cm-var")).not.toBeNull();
});

test("does not lint non-JSON bodies", async () => {
  const { container } = mount({ value: "<a><b></a>", language: "xml" });
  await tick();
  await new Promise((resolve) => setTimeout(resolve, 400));
  expect(container.querySelector("p[aria-live='polite']")?.textContent).toBe(
    "",
  );
  expect(container.querySelector(".cm-lintRange, .cm-lint-marker")).toBeNull();
  expect(container.querySelector(".cm-gutter-lint")).toBeNull();
});

const isMac = /Mac|iP(hone|ad)/.test(navigator.platform);

function press(target: HTMLElement, key: string) {
  const event = new KeyboardEvent("keydown", {
    key,
    metaKey: isMac,
    ctrlKey: !isMac,
    bubbles: true,
    cancelable: true,
  });
  target.dispatchEvent(event);
  return event;
}

test("mod_enter_is_left_to_the_app", () => {
  mount({ value: "{}", label: "Body" });
  const content = screen.getByRole("textbox", { name: "Body" });
  expect(press(content, "Enter").defaultPrevented).toBe(false);
});

test("search_opens_with_mod_f_when_enabled", () => {
  const { container } = mount({ value: "{}", search: true });
  const content = screen.getByRole("textbox", { name: "Request body" });
  press(content, "f");
  expect(container.querySelector(".cm-search")).not.toBeNull();
});

test("search_is_off_by_default", () => {
  const { container } = mount({ value: "{}" });
  const content = screen.getByRole("textbox", { name: "Request body" });
  press(content, "f");
  expect(container.querySelector(".cm-search")).toBeNull();
});

test("folding_adds_a_gutter_when_enabled", () => {
  const { container } = mount({ value: "{}", fold: true });
  expect(container.querySelector(".cm-foldGutter")).not.toBeNull();
});

test("folding_is_off_by_default", () => {
  const { container } = mount({ value: "{}" });
  expect(container.querySelector(".cm-foldGutter")).toBeNull();
});
