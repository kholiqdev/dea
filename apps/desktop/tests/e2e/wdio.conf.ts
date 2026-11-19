// End-to-end tests drive the debug build through tauri-driver, on Linux
// (WebKitWebDriver) and Windows (msedgedriver). macOS has no WebDriver for
// WKWebView.

import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const windows = process.platform === "win32";
const repo = resolve(import.meta.dirname, "../../../..");
const application = join(
  repo,
  "target",
  "debug",
  windows ? "dea-desktop.exe" : "dea-desktop",
);
const tauriDriver = join(
  process.env.CARGO_HOME ?? join(homedir(), ".cargo"),
  "bin",
  windows ? "tauri-driver.exe" : "tauri-driver",
);

// One driver and one empty app data folder per run. Both are made in
// onPrepare and removed in onComplete, which run in the launcher process
// (beforeSession runs in a worker, so its handles never reach onComplete).
// A restart inside a spec (browser.reloadSession) keeps both, so saved
// state survives it.
let driver: ChildProcess | undefined;
let dataDir: string | undefined;

const DRIVER_PORT = 4444;

/** Screenshots and page source of failed specs, kept by CI. */
const ARTIFACTS = join(repo, "target", "e2e");

function portOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = connect(port, "127.0.0.1");
    socket.once("connect", () => {
      socket.destroy();
      resolve(true);
    });
    socket.once("error", () => resolve(false));
  });
}

/** Waits until tauri-driver accepts connections; a fresh exe can be slow. */
async function waitForDriver(timeoutMs: number): Promise<void> {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (driver?.exitCode != null)
      throw new Error("tauri-driver exited at start");
    if (await portOpen(DRIVER_PORT)) return;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`tauri-driver did not listen on ${DRIVER_PORT}`);
}

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: DRIVER_PORT,
  specs: ["./**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": { application },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "warn",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 120_000 },
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 3,

  async afterTest(test, _context, result) {
    if (result.passed) return;
    mkdirSync(ARTIFACTS, { recursive: true });
    const name = test.title.replace(/[^a-z0-9]+/gi, "-");
    await browser.saveScreenshot(join(ARTIFACTS, `${name}.png`));
    writeFileSync(
      join(ARTIFACTS, `${name}.html`),
      await browser.getPageSource(),
    );
  },

  async onPrepare() {
    if (await portOpen(DRIVER_PORT))
      throw new Error(
        `Port ${DRIVER_PORT} is in use: stop the tauri-driver left from another run`,
      );
    dataDir = mkdtempSync(join(tmpdir(), "dea-e2e-"));
    // Workers start after this and inherit both: specs read the app data
    // and pick folders through them.
    process.env.DEA_APP_DATA_DIR = dataDir;
    process.env.DEA_E2E_PICK_FILE = join(dataDir, "pick-folder.txt");
    driver = spawn(tauriDriver, [], {
      stdio: ["ignore", "inherit", "inherit"],
      env: { ...process.env, DEA_E2E: "1" },
    });
    await waitForDriver(60_000);
  },

  onComplete() {
    driver?.kill();
    if (dataDir) rmSync(dataDir, { recursive: true, force: true });
  },
};
