// A module manifest: data plus lazy loaders. The shell reads manifests
// only through the registry, so a module's code loads when first used.

import type { Component } from "svelte";
import type {
  CommandData,
  IntentData,
  ManifestData,
  PanelData,
  SettingsSectionData,
  StatusItemData,
  TabTypeData,
} from "./manifest-data";

export type {
  Action,
  CommandData,
  RailData,
  ShortcutData,
} from "./manifest-data";

export type Loader<T> = () => Promise<{ default: T }>;

// Views take different props, so the registry stores them loosely typed.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyComponent = Component<any>;

export type TabTypeContribution = TabTypeData & { view: Loader<AnyComponent> };
export type PanelContribution = PanelData & { view: Loader<AnyComponent> };
export type StatusItemContribution = StatusItemData & {
  view: Loader<AnyComponent>;
};
export type SettingsSectionContribution = SettingsSectionData & {
  view: Loader<AnyComponent>;
};
export type IntentContribution = IntentData & {
  handler: Loader<(args: unknown) => unknown>;
};

export interface ModuleManifest extends Omit<
  ManifestData,
  "tabTypes" | "panels" | "statusItems" | "settings" | "intents"
> {
  /** The module's main view. */
  view?: Loader<AnyComponent>;
  tabTypes?: TabTypeContribution[];
  panels?: PanelContribution[];
  statusItems?: StatusItemContribution[];
  settings?: SettingsSectionContribution[];
  intents?: IntentContribution[];
}

/** A contribution together with the module that made it. */
export type FromModule<T> = T & { module: string };

export type Command = FromModule<CommandData>;
