import { screen, within } from "@testing-library/svelte";
import { expect, test } from "vitest";
import { fakeIpc } from "./shell/test-support";
import { start } from "./start";

test("starts with the built-in modules and the Welcome view", async () => {
  const log = fakeIpc();
  const target = document.body.appendChild(document.createElement("div"));
  await start(target);

  const rail = screen.getByRole("navigation", { name: "Modules" });
  for (const name of [
    "Requests",
    "Database",
    "Terminal",
    "Mobile",
    "Vault",
    "Settings",
  ])
    expect(within(rail).getByRole("button", { name })).toBeInTheDocument();
  expect(
    await screen.findByRole("heading", { name: "Welcome" }),
  ).toBeInTheDocument();
  expect(document.documentElement.dataset.theme).toBe("light");
  expect(log.savedUiStates).toEqual([]);
});
