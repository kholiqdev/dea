// The data half of a module manifest: everything except the loaders. This
// is the source of manifest.schema.json, which an external module's
// manifest must match (F1-FON-18).

/** A contribution id: `<module>.<name>`, for example `requests.open`. */
export type ContributionId = string;

/** What running a command does. */
export type Action =
  | { type: "openModule"; module: string }
  | { type: "openSettings"; section: string }
  | { type: "intent"; intent: string; args?: unknown };

export interface RailData {
  /** Icon name, mapped to an icon by the shell. */
  icon: string;
  group: "main" | "bottom";
  order: number;
}

export interface TabTypeData {
  id: ContributionId;
  title: string;
  icon: string;
  /**
   * `"window"`: the tab shows the window's active environment, and follows
   * it when it changes.
   */
  environment?: "window";
}

export interface PanelData {
  id: ContributionId;
  /** The module whose sidebar shows the panel. */
  module: string;
  title: string;
}

export interface CommandData {
  id: ContributionId;
  title: string;
  action: Action;
  /**
   * `project`: the command needs the window's project, so it is hidden
   * while the window shows Welcome, even in an app-wide module.
   */
  scope?: "project";
}

export interface ShortcutData {
  /** The command the chord runs. */
  command: ContributionId;
  /** A chord such as `Mod+Shift+T`. */
  chord: string;
}

export interface StatusItemData {
  id: ContributionId;
  align: "start" | "end";
  order: number;
}

export interface SettingsSectionData {
  id: ContributionId;
  title: string;
  order: number;
}

export interface IntentData {
  intent: string;
}

/** A source of command palette items, such as a module's environments. */
export interface SearchSourceData {
  id: ContributionId;
  /** The palette group its items show under, such as `Environments`. */
  group: string;
  /** Where the group sits in the palette: lower first. */
  order: number;
}

export interface ManifestData {
  /** The module id, for example `requests`. */
  id: string;
  title: string;
  /**
   * `project` (the default): works on the window's project, so it is off
   * while the window shows Welcome. `app`: always available.
   */
  scope?: "app" | "project";
  rail?: RailData;
  /**
   * `shell` (the default): the shell draws the sidebar's heading, the module
   * title. `module`: the module's sidebar draws its own head.
   */
  sidebarHeader?: "shell" | "module";
  tabTypes?: TabTypeData[];
  panels?: PanelData[];
  commands?: CommandData[];
  shortcuts?: ShortcutData[];
  statusItems?: StatusItemData[];
  settings?: SettingsSectionData[];
  intents?: IntentData[];
  searchSources?: SearchSourceData[];
}
