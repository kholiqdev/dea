// The registry and shell state the shell components share.

import { createContext } from "svelte";
import type { Registry } from "../contributions/registry.svelte";
import type { Tabs } from "../tabs/tabs.svelte";
import type { Shell } from "./shell-state.svelte";

export interface ShellContext {
  readonly registry: Registry;
  readonly shell: Shell;
  readonly tabs: Tabs;
}

/** Reads the shell context; throws outside the shell. */
export const [shellContext, setShellContext] = createContext<ShellContext>();
