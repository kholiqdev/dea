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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Settings {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Appearance {
    pub theme: ThemePreference,
    pub ui_font: String,
    pub mono_font: String,
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
    pub version: u32,
    pub active_module: Option<String>,
    pub rail_expanded: bool,
    /// Sidebar state per module id.
    pub sidebars: BTreeMap<String, SidebarState>,
    pub tabs: Vec<TabState>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct SidebarState {
    pub visible: bool,
    pub width: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct TabState {
    pub id: String,
    pub tab_type: String,
    pub title: String,
    /// Whatever the tab type needs to open the tab again.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub params: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Invalid(String),
    #[error("cannot write {}: {source}", path.display())]
    Io {
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
    /// Does no I/O: folders are created on the first save.
    pub fn new(app_data: &Path) -> Store {
        Store {
            dir: app_data.to_path_buf(),
        }
    }

    pub fn settings(&self) -> Settings {
        read_or_default(&self.dir.join(SETTINGS_FILE), |s: &Settings| {
            s.version <= SETTINGS_VERSION && validate(s).is_ok()
        })
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), StoreError> {
        validate(settings)?;
        let settings = Settings {
            version: SETTINGS_VERSION,
            ..settings.clone()
        };
        write_json(&self.dir.join(SETTINGS_FILE), &settings)
    }

    pub fn ui_state(&self, key: &str) -> UiState {
        read_or_default(&self.ui_path(key), |s: &UiState| {
            s.version <= UI_STATE_VERSION
        })
    }

    pub fn save_ui_state(&self, key: &str, state: &UiState) -> Result<(), StoreError> {
        let mut state = UiState {
            version: UI_STATE_VERSION,
            ..state.clone()
        };
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
