import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import App from "./App.svelte";

test("renders the product name", () => {
  render(App);
  expect(screen.getByRole("heading", { name: "Dea" })).toBeInTheDocument();
});
