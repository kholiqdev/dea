// End-to-end tests drive the debug build through tauri-driver, on Linux
// (WebKitWebDriver) and Windows (msedgedriver). macOS has no WebDriver for
// WKWebView.

import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
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

// One driver and one empty app data folder per run. A restart inside a
// spec (browser.reloadSession) keeps both, so saved state survives it.
let driver: ChildProcess | undefined;
let dataDir: string | undefined;

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
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

  beforeSession() {
    if (driver) return;
    dataDir = mkdtempSync(join(tmpdir(), "dea-e2e-"));
    driver = spawn(tauriDriver, [], {
      stdio: ["ignore", "inherit", "inherit"],
      env: { ...process.env, DEA_E2E: "1", DEA_APP_DATA_DIR: dataDir },
    });
  },

  onComplete() {
    driver?.kill();
    if (dataDir) rmSync(dataDir, { recursive: true, force: true });
  },
};
