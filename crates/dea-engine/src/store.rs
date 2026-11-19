//! Settings and UI state, kept as JSON files in the app data folder.
//!
//! A file that cannot be read, or that a newer Dea wrote, is set aside as
//! `<name>.damaged` and the defaults are used, so Dea always opens.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The settings file format this Dea writes.
pub const SETTINGS_VERSION: u32 = 1;
/// The UI state file format this Dea writes.
pub const UI_STATE_VERSION: u32 = 1;
/// How many closed tabs are kept for reopening.
pub const MAX_CLOSED_TABS: usize = 20;
/// The font sizes Settings offers, in pixels.
pub const FONT_SIZES: RangeInclusive<u8> = 11..=20;

const SETTINGS_FILE: &str = "settings.json";
const UI_DIR: &str = "ui";

/// The person's settings, shared by every project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Settings {
    /// The file format; saving always writes [`SETTINGS_VERSION`].
    pub version: u32,
    pub appearance: Appearance,
    /// Shortcut overrides by command id. `None` turns a shortcut off; a
    /// command that is not listed keeps its default.
    pub shortcuts: BTreeMap<String, Option<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            appearance: Appearance::default(),
            shortcuts: BTreeMap::new(),
        }
    }
}

/// Theme, fonts and text size; Settings applies them without a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Appearance {
    pub theme: ThemePreference,
    /// The interface font, by name.
    pub ui_font: String,
    /// The font of editors, terminals, tables and logs, by name.
    pub mono_font: String,
    /// Text size in pixels, within [`FONT_SIZES`].
    pub font_size: u8,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            ui_font: "IBM Plex Sans".into(),
            mono_font: "IBM Plex Mono".into(),
            font_size: 13,
        }
    }
}

/// Which theme to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum ThemePreference {
    Dark,
    Light,
    /// Follow the OS, also when it changes while Dea runs.
    #[default]
    System,
}

/// What the window looked like, so it opens the same way next time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct UiState {
    /// The file format; saving always writes [`UI_STATE_VERSION`].
    pub version: u32,
    /// The module shown in the work area, by id.
    pub active_module: Option<String>,
    /// Whether the rail shows module names.
    pub rail_expanded: bool,
    /// Sidebar state per module id.
    pub sidebars: BTreeMap<String, SidebarState>,
    /// Open tabs, in tab bar order.
    pub tabs: Vec<TabState>,
    /// The id of the active tab.
    pub active_tab: Option<String>,
    /// Recently closed tabs, newest last.
    pub closed_tabs: Vec<TabState>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            version: UI_STATE_VERSION,
            active_module: None,
            rail_expanded: false,
            sidebars: BTreeMap::new(),
            tabs: Vec::new(),
            active_tab: None,
            closed_tabs: Vec::new(),
        }
    }
}

/// One module's sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct SidebarState {
    pub visible: bool,
    /// Width in pixels, as last set; the UI fits it to the window.
    pub width: u16,
}

/// A tab to open again after a restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct TabState {
    /// Kept across restarts, so the active tab can be found again.
    pub id: String,
    /// The contribution id of the tab type, for example `requests.request`.
    pub tab_type: String,
    /// The text on the tab.
    pub title: String,
    /// Whatever the tab type needs to open the tab again.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub params: serde_json::Value,
}

/// Why settings or UI state could not be saved.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The value breaks a rule, such as a font size outside [`FONT_SIZES`].
    /// The message is meant to be shown to the person as it is.
    #[error("{0}")]
    Invalid(String),
    /// The file or its folder could not be written.
    #[error("cannot write {}", path.display())]
    Io {
        /// The file or folder that could not be written.
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// The settings and UI state files in one app data folder.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// A store in the `app_data` folder. Does no I/O: folders are created
    /// on the first save.
    #[must_use]
    pub fn new(app_data: impl Into<PathBuf>) -> Store {
        Store {
            dir: app_data.into(),
        }
    }

    /// Reads the settings, or the defaults when there are none yet. A file
    /// that is damaged, invalid or from a newer Dea is set aside as
    /// `settings.json.damaged`.
    pub fn load_settings(&self) -> Settings {
        read_or_default(&self.dir.join(SETTINGS_FILE), |s: &Settings| {
            s.version <= SETTINGS_VERSION && validate(s).is_ok()
        })
    }

    /// Saves the settings in this Dea's format.
    ///
    /// # Errors
    ///
    /// [`StoreError::Invalid`] when the font size is outside [`FONT_SIZES`]
    /// or a font name is empty; nothing is written then.
    /// [`StoreError::Io`] when the file or its folder cannot be written.
    pub fn save_settings(&self, mut settings: Settings) -> Result<(), StoreError> {
        validate(&settings)?;
        settings.version = SETTINGS_VERSION;
        write_json(&self.dir.join(SETTINGS_FILE), &settings)
    }

    /// Reads the UI state saved under `key`, or the defaults. Damaged files
    /// are set aside as for [`Store::load_settings`].
    pub fn load_ui_state(&self, key: &str) -> UiState {
        read_or_default(&self.ui_path(key), |s: &UiState| {
            s.version <= UI_STATE_VERSION
        })
    }

    /// Saves the UI state under `key`, keeping the newest
    /// [`MAX_CLOSED_TABS`] closed tabs.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] when the file or its folder cannot be written.
    pub fn save_ui_state(&self, key: &str, mut state: UiState) -> Result<(), StoreError> {
        state.version = UI_STATE_VERSION;
        let extra = state.closed_tabs.len().saturating_sub(MAX_CLOSED_TABS);
        state.closed_tabs.drain(..extra);
        write_json(&self.ui_path(key), &state)
    }

    /// The key never appears in the file name, since it may name a project.
    fn ui_path(&self, key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        let name: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join(UI_DIR).join(format!("{name}.json"))
    }
}

fn validate(s: &Settings) -> Result<(), StoreError> {
    let a = &s.appearance;
    if !FONT_SIZES.contains(&a.font_size) {
        return Err(StoreError::Invalid(format!(
            "Font size must be {} to {}",
            FONT_SIZES.start(),
            FONT_SIZES.end()
        )));
    }
    if a.ui_font.trim().is_empty() || a.mono_font.trim().is_empty() {
        return Err(StoreError::Invalid("A font name cannot be empty".into()));
    }
    Ok(())
}

/// Reads `path`, or gives the defaults when there is no file. A file that
/// cannot be read or parsed (not UTF-8, not JSON, locked), or that `usable`
/// rejects, is renamed to `<name>.damaged` so the next save cannot
/// overwrite it.
fn read_or_default<T: DeserializeOwned + Default>(path: &Path, usable: impl Fn(&T) -> bool) -> T {
    let value = match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(_) => None,
        Ok(bytes) => serde_json::from_slice::<T>(&bytes)
            .ok()
            .filter(|v| usable(v)),
    };
    value.unwrap_or_else(|| {
        let mut damaged = path.as_os_str().to_owned();
        damaged.push(".damaged");
        // If even the rename fails, the next save replaces the file.
        let _ = fs::rename(path, PathBuf::from(damaged));
        T::default()
    })
}

/// Replaces `path` through a temporary file in the same folder, so a crash
/// never leaves half a file. On Unix the file is 0600 and new folders 0700.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let io_err = |path: &Path| {
        let path = path.to_path_buf();
        move |source| StoreError::Io { path, source }
    };
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir).map_err(io_err(dir))?;
    let text =
        serde_json::to_string_pretty(value).map_err(|e| StoreError::Invalid(e.to_string()))? + "\n";
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(io_err(dir))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(io_err(tmp.path()))?;
    }
    tmp.write_all(text.as_bytes()).map_err(io_err(tmp.path()))?;
    tmp.as_file().sync_all().map_err(io_err(tmp.path()))?;
    tmp.persist(path).map_err(|e| io_err(path)(e.error))?;
    Ok(())
}
