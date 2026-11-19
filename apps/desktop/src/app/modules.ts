// The built-in modules. Each one reaches the shell only through its
// manifest; views load the first time they are opened.

import type { Component } from "svelte";
import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
import Database from "@lucide/svelte/icons/database";
import KeyRound from "@lucide/svelte/icons/key-round";
import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
import Smartphone from "@lucide/svelte/icons/smartphone";
import SquareTerminal from "@lucide/svelte/icons/square-terminal";
import type { Registry } from "./contributions/registry.svelte";
import type { ModuleManifest } from "./contributions/types";
import { manifest as database } from "../modules/database";
import { manifest as mobile } from "../modules/mobile";
import { manifest as project } from "../modules/project";
import { manifest as requests } from "../modules/requests";
import { manifest as settings } from "../modules/settings";
import { manifest as terminal } from "../modules/terminal";
import { manifest as vault } from "../modules/vault";

export const BUILT_IN_MODULES: readonly ModuleManifest[] = [
  requests,
  database,
  terminal,
  mobile,
  vault,
  settings,
  project,
];

type Icon = Component<{ size?: number; strokeWidth?: number }>;

// Rail icons by the name a manifest gives.
const ICONS: Record<string, Icon> = {
  "arrow-left-right": ArrowLeftRight,
  database: Database,
  "square-terminal": SquareTerminal,
  smartphone: Smartphone,
  "key-round": KeyRound,
  "sliders-horizontal": SlidersHorizontal,
};

export function railIcon(name: string): Icon | undefined {
  return ICONS[name];
}

export function registerBuiltInModules(r: Registry): void {
  for (const m of BUILT_IN_MODULES) r.register(m);
}
