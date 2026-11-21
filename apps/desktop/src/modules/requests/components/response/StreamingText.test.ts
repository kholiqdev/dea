import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import StreamingText from "./StreamingText.svelte";

test("chunked_text_appears_while_it_arrives", async () => {
  const view = render(StreamingText, {
    text: "part one",
    textBytes: 8,
    received: 8,
  });
  expect(screen.getByText("part one")).toBeInTheDocument();
  expect(screen.getByText("Receiving… 8 B")).toBeInTheDocument();
  await view.rerender({
    text: "part one, part two",
    textBytes: 18,
    received: 18,
  });
  expect(screen.getByText("part one, part two")).toBeInTheDocument();
});

test("text_past_the_limit_says_it_is_shown_when_complete", () => {
  render(StreamingText, {
    text: "abc",
    textBytes: 3,
    received: 2 * 1024 * 1024,
  });
  expect(
    screen.getByText("Receiving… 2.0 MB Shown when it is complete."),
  ).toBeInTheDocument();
});

test("non_ascii_text_is_not_called_incomplete", () => {
  // Six bytes in three characters, all of them shown.
  render(StreamingText, { text: "ééé", textBytes: 6, received: 6 });
  expect(screen.getByText("Receiving… 6 B")).toBeInTheDocument();
  expect(screen.queryByText(/Shown when it is complete/)).toBeNull();
});
