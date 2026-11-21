import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import Button from "./Button.svelte";
import source from "./Button.svelte?raw";

test("danger_solid_uses_the_negative_solid_token", () => {
  render(Button, { variant: "danger-solid" });
  expect(screen.getByRole("button")).toHaveClass("danger-solid");
  const rule = /\.danger-solid\s*\{([^}]*)\}/.exec(source)?.[1] ?? "";
  expect(rule).toContain("background: var(--negative-solid)");
  expect(rule).toContain("color: var(--primary-text)");
  expect(rule).toContain("font-weight: 500");
});
