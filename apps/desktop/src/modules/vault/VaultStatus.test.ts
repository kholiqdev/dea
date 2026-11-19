import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import VaultStatusItem from "./components/VaultStatusItem.svelte";
import { fakeVault } from "./test-support";

test("the status item says locked or unlocked and opens the Vault", async () => {
  const openModule = vi.fn(() => Promise.resolve());
  setShellApi({ openModule } as unknown as ShellApi);

  fakeVault(true);
  const view = render(VaultStatusItem);
  const locked = screen.getByRole("button", { name: "Vault locked" });
  await userEvent.click(locked);
  expect(openModule).toHaveBeenCalledWith("vault");
  view.unmount();

  fakeVault(false);
  render(VaultStatusItem);
  expect(
    screen.getByRole("button", { name: "Vault unlocked" }),
  ).toBeInTheDocument();
});
