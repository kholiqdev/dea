import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { setProjectApi, type ProjectApi } from "$app/contributions/project-api";
import { setShellApi, type ShellApi } from "$app/contributions/shell-api";
import type { ProjectInfo } from "$lib/ipc/bindings/ProjectInfo";
import type { UnlockMethod } from "$lib/ipc/bindings/UnlockMethod";
import { setVaultApi, type VaultApi } from "$app/contributions/vault-api";
import { settings } from "$lib/settings/settings.svelte";
import SecuritySection from "./components/SecuritySection.svelte";
import { fakeSettingsIpc, resetSettings } from "./test-support";

let ipc: ReturnType<typeof fakeSettingsIpc>;
let runCommand: ReturnType<typeof vi.fn>;

function setup(opts: { project?: boolean; method?: UnlockMethod } = {}): void {
  const info = (
    opts.project === false ? null : { name: "Demo" }
  ) as ProjectInfo | null;
  setProjectApi({ info } as unknown as ProjectApi);
  setVaultApi({
    locked: true,
    method: opts.method ?? "deviceOwner",
  } as unknown as VaultApi);
  runCommand = vi.fn(() => Promise.resolve());
  setShellApi({ runCommand } as unknown as ShellApi);
}

beforeEach(async () => {
  ipc = fakeSettingsIpc();
  await resetSettings();
  ipc.saved.length = 0;
  setup();
});

test("Lock when idle and Ask again for production are saved", async () => {
  render(SecuritySection);
  const idle = screen.getByRole("spinbutton", {
    name: "Lock when idle, minutes",
  });
  const prod = screen.getByRole("spinbutton", {
    name: "Ask again for production, minutes",
  });
  expect(idle).toHaveValue(15);
  expect(prod).toHaveValue(10);

  await userEvent.clear(idle);
  await userEvent.type(idle, "30{Enter}");
  expect(settings.value.security.autoLockMinutes).toBe(30);
  expect(ipc.saved.at(-1)?.security.autoLockMinutes).toBe(30);

  await userEvent.clear(prod);
  await userEvent.type(prod, "5{Enter}");
  expect(ipc.saved.at(-1)?.security).toEqual({
    autoLockMinutes: 30,
    productionMinutes: 5,
  });
});

test("Never lock disables the minutes and saves null", async () => {
  render(SecuritySection);
  const idle = screen.getByRole("spinbutton", {
    name: "Lock when idle, minutes",
  });
  const never = screen.getByRole("checkbox", {
    name: "Never lock while Dea is open",
  });
  await userEvent.click(never);
  expect(idle).toBeDisabled();
  expect(ipc.saved.at(-1)?.security.autoLockMinutes).toBeNull();

  await userEvent.click(never);
  expect(idle).toBeEnabled();
  expect(ipc.saved.at(-1)?.security.autoLockMinutes).toBe(15);
});

test("a minute value outside its range is not saved", async () => {
  render(SecuritySection);
  const idle = screen.getByRole("spinbutton", {
    name: "Lock when idle, minutes",
  });
  const prod = screen.getByRole("spinbutton", {
    name: "Ask again for production, minutes",
  });
  for (const bad of ["0", "241", "1.5", "abc", ""]) {
    await userEvent.clear(idle);
    if (bad) await userEvent.type(idle, bad);
    await userEvent.tab();
    expect(idle).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Use a number from 1 to 240")).toBeInTheDocument();
    expect(idle).toHaveAccessibleDescription(/Use a number from 1 to 240/);
  }
  for (const bad of ["0", "61", ""]) {
    await userEvent.clear(prod);
    if (bad) await userEvent.type(prod, bad);
    await userEvent.tab();
    expect(prod).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Use a number from 1 to 60")).toBeInTheDocument();
  }
  expect(ipc.saved).toHaveLength(0);
  expect(settings.value.security).toEqual({
    autoLockMinutes: 15,
    productionMinutes: 10,
  });

  await userEvent.clear(prod);
  await userEvent.type(prod, "60{Enter}");
  expect(prod).not.toHaveAttribute("aria-invalid", "true");
  expect(ipc.saved.at(-1)?.security.productionMinutes).toBe(60);
});

test("the production hint names how this device asks", () => {
  setup({ method: "vaultPassword" });
  render(SecuritySection);
  expect(
    screen.getByText(
      "From 1 to 60. After this, production secrets ask for your Vault password again",
    ),
  ).toBeInTheDocument();
  expect(screen.getByText("From 1 to 240")).toBeInTheDocument();
  expect(
    screen.getByText("Dea keeps everything on this computer."),
  ).toBeInTheDocument();
});

test("Clear local data opens the project's dialog, and is hidden on Welcome", async () => {
  const view = render(SecuritySection);
  expect(
    screen.getByText(
      "Removes history, cookies, saved examples and window state on this computer. Project files and Vault stay",
    ),
  ).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "Clear local data…" }),
  );
  expect(runCommand).toHaveBeenCalledWith("shell.clearLocalData");
  view.unmount();

  setup({ project: false });
  render(SecuritySection);
  expect(screen.queryByText("Local data")).toBeNull();
  expect(
    screen.queryByRole("button", { name: "Clear local data…" }),
  ).toBeNull();
  expect(screen.getByText("Vault")).toBeInTheDocument();
});
