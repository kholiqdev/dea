// The registry and shell state the shell components share.

import { getContext, setContext } from "svelte";
import type { Registry } from "../contributions/registry.svelte";
import type { Shell } from "./shell-state.svelte";

export interface ShellContext {
  registry: Registry;
  shell: Shell;
}

const KEY = Symbol("shell");

export function setShellContext(ctx: ShellContext): void {
  setContext(KEY, ctx);
}

export function shellContext(): ShellContext {
  return getContext<ShellContext>(KEY);
}
