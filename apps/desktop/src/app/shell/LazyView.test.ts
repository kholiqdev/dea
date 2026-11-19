import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import LazyView from "./LazyView.svelte";
import { view } from "./test-support";

test("a new loader replaces the view", async () => {
  const { rerender } = render(LazyView, { loader: view("first") });
  expect(await screen.findByText("first")).toBeInTheDocument();
  await rerender({ loader: view("second") });
  expect(await screen.findByText("second")).toBeInTheDocument();
  expect(screen.queryByText("first")).not.toBeInTheDocument();
});
