import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import StreamingText from "./StreamingText.svelte";

test("chunked_text_appears_while_it_arrives", async () => {
  const view = render(StreamingText, { text: "part one", received: 8 });
  expect(screen.getByText("part one")).toBeInTheDocument();
  expect(screen.getByText("Receiving… 8 B")).toBeInTheDocument();
  await view.rerender({ text: "part one, part two", received: 18 });
  expect(screen.getByText("part one, part two")).toBeInTheDocument();
});

test("text_past_the_limit_says_it_is_shown_when_complete", () => {
  render(StreamingText, { text: "abc", received: 2 * 1024 * 1024 });
  expect(
    screen.getByText("Receiving… 2.0 MB Shown when it is complete."),
  ).toBeInTheDocument();
});
