import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import MethodBadge from "./MethodBadge.svelte";

test("standard methods use their token and custom ones the neutral text", () => {
  render(MethodBadge, { method: "PATCH" });
  expect(screen.getByText("PATCH").getAttribute("style")).toContain(
    "color: var(--method-patch)",
  );
  render(MethodBadge, { method: "PROPFIND" });
  const custom = screen.getByText("PROPFIN…");
  expect(custom).toHaveAttribute("title", "PROPFIND");
  expect(custom.getAttribute("style")).toContain("color: var(--text-2)");
});
