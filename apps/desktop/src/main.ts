import { invoke } from "@tauri-apps/api/core";
import "./styles/global.css";
import { start } from "./app/start";

const app = await start(document.getElementById("app")!);

// Signals the performance probe once the UI is mounted and again after the
// first frame is drawn. Outside Tauri, for example in a plain browser, there
// is nothing to signal.
invoke("perf_mark", { name: "mounted" }).catch(() => {});
requestAnimationFrame(() =>
  requestAnimationFrame(() => {
    invoke("perf_ready").catch(() => {});
  }),
);

export default app;
