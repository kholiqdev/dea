import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { UnlockMethod } from "$lib/ipc/bindings/UnlockMethod";
import LockedVault from "./components/LockedVault.svelte";
import VaultFooter from "./components/VaultFooter.svelte";
import { fakeVault } from "./test-support";

const LABELS: [UnlockMethod, string][] = [
  ["deviceOwner", "Unlock with Touch ID"],
  ["windowsHello", "Unlock with Windows Hello"],
  ["windowsPassword", "Unlock with your Windows password"],
  ["vaultPassword", "Unlock with your Vault password"],
];

test("the locked screen names the way this device unlocks", async () => {
  for (const [method, label] of LABELS) {
    const vault = fakeVault(true, method);
    const view = render(LockedVault);
    expect(
      screen.getByRole("heading", { name: "Vault is locked" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Requests, hosts and connections that use a secret wait until you unlock. Nothing is sent while the vault is locked.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: label }));
    expect(vault.unlocked).toBe(1);
    view.unmount();
  }
});

test("the footer names the method and the idle time", () => {
  fakeVault(false, "deviceOwner");
  const view = render(VaultFooter, { autoLockMinutes: 15 });
  expect(
    screen.getByText("Unlocked with Touch ID, locks after 15 min idle"),
  ).toBeInTheDocument();
  expect(screen.getByText("Encrypted on this device")).toBeInTheDocument();
  view.unmount();

  render(VaultFooter, { autoLockMinutes: null });
  expect(
    screen.getByText(
      "Unlocked with Touch ID, stays unlocked while Dea is open",
    ),
  ).toBeInTheDocument();
});

test("a locked footer says Locked", () => {
  fakeVault(true);
  render(VaultFooter, { autoLockMinutes: 15 });
  expect(screen.getByText("Locked")).toBeInTheDocument();
  expect(screen.getByText("Encrypted on this device")).toBeInTheDocument();
});
