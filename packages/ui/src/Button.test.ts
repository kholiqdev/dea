import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import Button from "./Button.svelte";
import source from "./Button.svelte?raw";

test("danger_solid_uses_the_negative_solid_token", () => {
  render(Button, { variant: "danger-solid" });
  expect(screen.getByRole("button")).toHaveClass("danger-solid");
  const rule = /\.danger-solid\s*\{([^}]*)\}/.exec(source)?.[1] ?? "";
  expect(rule).toContain("background: var(--negative-solid)");
  expect(rule).toContain("font-weight: 500");
});

test("the_danger_solid_button_uses_its_own_text_token", () => {
  const rule = /\.danger-solid\s*\{([^}]*)\}/.exec(source)?.[1] ?? "";
  expect(rule).toContain("color: var(--negative-solid-text)");
});
