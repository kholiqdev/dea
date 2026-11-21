// A module manifest: data plus lazy loaders. The shell reads manifests
// only through the registry, so a module's code loads when first used.

import type { Component } from "svelte";
import type {
  CommandData,
  IntentData,
  ManifestData,
  PanelData,
  SearchSourceData,
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

/** What a tab shows in place of its icon, from the tab's params. */
export type TabBadge = { text: string; color: string };

export type TabTypeContribution = TabTypeData & {
  view: Loader<AnyComponent>;
  badge?: (params: unknown) => TabBadge | null;
};
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

/** One thing the command palette can find and open. */
export interface PaletteItem {
  /** Unique within its source. */
  id: string;
  title: string;
  meta?: string;
  /** The environment the item belongs to; production is marked. */
  environment?: { name: string; production: boolean };
  /** A command whose effective shortcut is shown next to the item. */
  command?: string;
  /** Whether the item opens a tab, so Mod+Enter can open a new one. */
  opensTab?: boolean;
  /** Opens the item in the active tab or a new one. */
  open(target: "active" | "new"): void | Promise<void>;
}

/** Lists a module's palette items when the palette opens. */
export type SearchSource = () => PaletteItem[] | Promise<PaletteItem[]>;

export type SearchSourceContribution = SearchSourceData & {
  source: Loader<SearchSource>;
};

export interface ModuleManifest extends Omit<
  ManifestData,
  | "tabTypes"
  | "panels"
  | "statusItems"
  | "settings"
  | "intents"
  | "searchSources"
> {
  /** The module's main view. */
  view?: Loader<AnyComponent>;
  /** What the module shows in the sidebar, if it has one. */
  sidebar?: Loader<AnyComponent>;
  tabTypes?: TabTypeContribution[];
  panels?: PanelContribution[];
  statusItems?: StatusItemContribution[];
  settings?: SettingsSectionContribution[];
  intents?: IntentContribution[];
  searchSources?: SearchSourceContribution[];
}

/** A contribution together with the module that made it. */
export type FromModule<T> = T & { module: string };

export type Command = FromModule<CommandData>;
