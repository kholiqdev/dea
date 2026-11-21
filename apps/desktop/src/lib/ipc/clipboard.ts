// Text the window already shows, put on the clipboard by Rust.

import { invoke } from "@tauri-apps/api/core";

export function copyText(text: string): Promise<void> {
  return invoke<void>("copy_text", { text });
}
