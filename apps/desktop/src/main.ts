import { invoke } from "@tauri-apps/api/core";
import { mount } from "svelte";
import "./styles/global.css";
import App from "./app/App.svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

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
