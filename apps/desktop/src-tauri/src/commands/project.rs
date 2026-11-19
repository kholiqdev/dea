//! Projects and their environments, one project per window. File work runs
//! off the main thread; each command passes its arguments on.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dea_engine::projects::{
    self, OpenedProject, ProjectChanged, ProjectInfo, RecentEntry, portable_path, recent_entries,
};
use dea_engine::vault::{VaultService, values_involved};
use dea_project::{
    EnvReference, Environment, Project, ProjectChange, ProjectError, ProjectWatcher, load_project,
};
use tauri::async_runtime::spawn_blocking;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

use crate::error::{IpcError, IpcErrorKind};
use crate::session::Session;
use crate::state::AppState;

/// The event a window gets when files of its project change on disk.
pub const PROJECT_CHANGED: &str = "project-changed";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) fn info_of(session: &Session) -> Result<ProjectInfo, IpcError> {
    if session.missing || !session.root.join("dea.yaml").is_file() {
        return Ok(ProjectInfo::missing(
            &session.root,
            &session.scope,
            session.slot,
        ));
    }
    let loaded = match load_project(&session.root) {
        Ok(loaded) => loaded,
        Err(ProjectError::InvalidManifest(e)) => {
            return Ok(ProjectInfo::unreadable(
                &session.root,
                &session.scope,
                session.slot,
                &e,
            ));
        }
        Err(e) => return Err(e.into()),
    };
    let opened = OpenedProject {
        project: loaded.project,
        errors: loaded.errors,
        migrated: None,
    };
    Ok(ProjectInfo::new(&opened, session.slot))
}

/// Gives `window` the opened project: a slot, a watcher and the first place
/// in the recent list.
fn bind(app: &AppHandle, window: &str, opened: &OpenedProject) -> ProjectInfo {
    let state = app.state::<AppState>();
    let project = &opened.project;
    let slot = state
        .sessions()
        .bind(window, project.root.clone(), project.vault_scope());
    // Failing to remember the project is no reason to refuse opening it.
    let _ = state.store.touch_recent(project, now());
    state.set_watcher(window, watch(app, window, &project.root));
    ProjectInfo::new(opened, slot)
}

fn unbind(app: &AppHandle, window: &str) {
    let state = app.state::<AppState>();
    state.set_watcher(window, None);
    state.sessions().unbind(window);
}

fn watch(app: &AppHandle, window: &str, root: &Path) -> Option<ProjectWatcher> {
    let app = app.clone();
    let label = window.to_string();
    ProjectWatcher::start(root, move |changes| on_changes(&app, &label, &changes)).ok()
}

/// One reload and one event per batch of changes, however many files.
fn on_changes(app: &AppHandle, window: &str, changes: &[ProjectChange]) {
    let state = app.state::<AppState>();
    let session = {
        let mut sessions = state.sessions();
        sessions.refresh_missing(window);
        sessions.get(window).cloned()
    };
    let Some(session) = session else {
        return;
    };
    if session.missing {
        state.set_watcher(window, None);
    }
    let info = info_of(&session)
        .unwrap_or_else(|_| ProjectInfo::missing(&session.root, &session.scope, session.slot));
    let changed = changes.iter().map(|c| portable_path(&c.path)).collect();
    let _ = app.emit_to(window, PROJECT_CHANGED, ProjectChanged { info, changed });
}

/// Opens the most recent project in the first window, when its folder is
/// still there.
pub fn reopen_last(app: &AppHandle, window: &str) {
    let state = app.state::<AppState>();
    let Some(last) = state.store.load_recent().into_iter().next() else {
        return;
    };
    if let Ok(opened) = projects::open_project(&last.root) {
        bind(app, window, &opened);
    }
}

/// Forgets the project of a closed window and cancels its Vault password
/// prompts.
pub fn window_closed(app: &AppHandle, window: &str) {
    app.state::<AppState>()
        .vault
        .prompter()
        .cancel_window(window);
    unbind(app, window);
}

pub(crate) fn writable(app: &AppHandle, window: &str) -> Result<Session, IpcError> {
    Ok(app.state::<AppState>().sessions().writable(window)?.clone())
}

#[tauri::command]
pub async fn window_project(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<ProjectInfo>, IpcError> {
    let state = app.state::<AppState>();
    let (session, came_back) = {
        let mut sessions = state.sessions();
        let was_missing = sessions.get(window.label()).is_some_and(|s| s.missing);
        let missing = sessions.refresh_missing(window.label());
        (
            sessions.get(window.label()).cloned(),
            was_missing && !missing,
        )
    };
    let Some(session) = session else {
        return Ok(None);
    };
    // A folder that came back is watched again.
    if came_back {
        state.set_watcher(window.label(), watch(&app, window.label(), &session.root));
    }
    Ok(Some(spawn_blocking(move || info_of(&session)).await??))
}

#[tauri::command]
pub async fn open_project(
    app: AppHandle,
    window: WebviewWindow,
    root: String,
) -> Result<ProjectInfo, IpcError> {
    let opened = spawn_blocking(move || projects::open_project(Path::new(&root))).await??;
    Ok(bind(&app, window.label(), &opened))
}

#[tauri::command]
pub async fn create_project(
    app: AppHandle,
    window: WebviewWindow,
    location: String,
    name: String,
) -> Result<ProjectInfo, IpcError> {
    let opened =
        spawn_blocking(move || projects::new_project(Path::new(&location), &name)).await??;
    Ok(bind(&app, window.label(), &opened))
}

/// Opens a new window: on the project at `root`, or on Welcome.
#[tauri::command]
pub async fn open_window(app: AppHandle, root: Option<String>) -> Result<(), IpcError> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let label = format!("project-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    if let Some(root) = root {
        let opened = spawn_blocking(move || projects::open_project(Path::new(&root))).await??;
        bind(&app, &label, &opened);
    }
    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("Dea")
        .inner_size(1280.0, 800.0)
        .min_inner_size(1024.0, 640.0)
        .build();
    if let Err(e) = built {
        unbind(&app, &label);
        return Err(e.into());
    }
    Ok(())
}

#[tauri::command]
pub fn close_project(app: AppHandle, window: WebviewWindow) {
    unbind(&app, window.label());
}

#[tauri::command]
pub async fn pick_folder(window: WebviewWindow) -> Result<Option<String>, IpcError> {
    if let Some(path) = crate::state::e2e_picked_folder() {
        return Ok(Some(path));
    }
    let picked = spawn_blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .blocking_pick_folder()
    })
    .await?;
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p: PathBuf| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn recent_projects(app: AppHandle) -> Result<Vec<RecentEntry>, IpcError> {
    let store = app.state::<AppState>().store.clone();
    Ok(spawn_blocking(move || recent_entries(&store.load_recent())).await?)
}

#[tauri::command]
pub async fn remove_recent(app: AppHandle, root: String) -> Result<(), IpcError> {
    let store = app.state::<AppState>().store.clone();
    Ok(spawn_blocking(move || store.remove_recent(Path::new(&root))).await??)
}

#[tauri::command]
pub async fn save_environment(
    app: AppHandle,
    window: WebviewWindow,
    previous: Option<String>,
    environment: Environment,
) -> Result<ProjectInfo, IpcError> {
    let session = writable(&app, window.label())?;
    let opened = app.state::<AppState>().vault.existing();
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        // One refusal is enough when a rename also turns production off.
        if production_turned_off(&project, previous.as_deref(), &environment) {
            require_production_open(
                opened.as_deref(),
                "turning production off needs authentication first",
            )?;
        } else if renames_production(&project, previous.as_deref(), &environment) {
            require_production_open(
                opened.as_deref(),
                "renaming a production environment needs authentication first",
            )?;
        }
        // A rename takes the environment's Vault values along, so none stay
        // behind under the freed name. Opening the Vault reaches the
        // keychain, so it is opened only when there are values to move.
        let rename = renamed(previous.as_deref(), &environment);
        let app_vault = &app.state::<AppState>().vault;
        let vault = match rename {
            Some((from, to)) => vault_for_values(
                &project,
                from,
                Some(to),
                |name| app_vault.has_values(&project, name),
                || app_vault.get_or_open(&app),
                RENAME_LOCKED,
            )?,
            None => None,
        };
        dea_project::save_environment(&project, previous.as_deref(), &environment)?;
        if let (Some(vault), Some((from, to))) = (vault, rename) {
            vault.move_environment(&project, from, to)?;
        }
        info_of(&session)
    })
    .await?
}

/// Whether saving `environment` over `previous` turns production off. Only
/// allowed while the production window is open (F1-FON-07 #4).
fn production_turned_off(
    project: &Project,
    previous: Option<&str>,
    environment: &Environment,
) -> bool {
    !environment.production && previous.is_some_and(|previous| marked_production(project, previous))
}

/// Whether saving `environment` over `previous` renames a production
/// environment. Only allowed while the production window is open: an
/// environment created under the freed name, not marked production, would
/// reach its Vault values.
fn renames_production(
    project: &Project,
    previous: Option<&str>,
    environment: &Environment,
) -> bool {
    previous.is_some_and(|previous| {
        previous != environment.name && marked_production(project, previous)
    })
}

/// Whether deleting `name` removes a production environment. Only allowed
/// while the production window is open, for the same reason as a rename.
fn deletes_production(project: &Project, name: &str) -> bool {
    marked_production(project, name)
}

/// Whether the project, as loaded, has an environment `name` marked
/// production.
fn marked_production(project: &Project, name: &str) -> bool {
    project
        .environments
        .iter()
        .any(|e| e.name == name && e.production)
}

/// Why a rename waits for the Vault: the environment's values move with it.
const RENAME_LOCKED: &str = "unlock the Vault to rename an environment that has secrets";
/// Why a delete waits for the Vault: the environment's values go with it.
const DELETE_LOCKED: &str = "unlock the Vault to delete an environment that has secrets";

/// The old and new name when saving `environment` over `previous` renames
/// an environment on disk.
fn renamed<'a>(
    previous: Option<&'a str>,
    environment: &'a Environment,
) -> Option<(&'a str, &'a str)> {
    previous
        .filter(|previous| *previous != environment.name)
        .map(|previous| (previous, environment.name.as_str()))
}

/// What a rename or delete needs from the Vault before the project is
/// written. A trait so tests can stand in for the Vault.
trait ValuesVault {
    /// Whether the Vault is locked.
    fn locked(&self) -> bool;
    /// Starts the idle timer again.
    fn record_activity(&self);
}

impl ValuesVault for VaultService {
    fn locked(&self) -> bool {
        self.status().locked
    }

    fn record_activity(&self) {
        VaultService::record_activity(self);
    }
}

/// The Vault to move or remove the values of a renamed or deleted
/// `environment` with (`to` is the new name of a rename), or `None` when no
/// values are involved, as [`values_involved`] decides with `has_values`.
/// Only then is the Vault opened with `open`, so a rename or delete without
/// secrets never reaches the keychain.
///
/// Refuses with `vaultLocked` and `message` while the Vault is locked.
/// Checked before the project is written, so a refusal changes nothing.
/// Otherwise the idle timer starts again, so the Vault cannot lock between
/// this check and the move.
fn vault_for_values<V: ValuesVault>(
    project: &Project,
    environment: &str,
    to: Option<&str>,
    has_values: impl Fn(&str) -> bool,
    open: impl FnOnce() -> Result<Arc<V>, IpcError>,
    message: &str,
) -> Result<Option<Arc<V>>, IpcError> {
    if !values_involved(project, environment, to, has_values) {
        return Ok(None);
    }
    let vault = open()?;
    if vault.locked() {
        return Err(IpcError {
            kind: IpcErrorKind::VaultLocked,
            message: message.into(),
        });
    }
    vault.record_activity();
    Ok(Some(vault))
}

/// Refuses with `reauthRequired` and `message` unless the production window
/// is open.
fn require_production_open(vault: Option<&VaultService>, message: &str) -> Result<(), IpcError> {
    if vault.is_some_and(VaultService::production_open) {
        return Ok(());
    }
    Err(IpcError {
        kind: IpcErrorKind::ReauthRequired,
        message: message.into(),
    })
}

#[tauri::command]
pub async fn delete_environment(
    app: AppHandle,
    window: WebviewWindow,
    name: String,
) -> Result<ProjectInfo, IpcError> {
    let session = writable(&app, window.label())?;
    let opened = app.state::<AppState>().vault.existing();
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        if deletes_production(&project, &name) {
            require_production_open(
                opened.as_deref(),
                "deleting a production environment needs authentication first",
            )?;
        }
        // The environment's Vault values go with it, so none stay behind
        // under the freed name. Opening the Vault reaches the keychain, so
        // it is opened only when there are values to remove.
        let app_vault = &app.state::<AppState>().vault;
        let vault = vault_for_values(
            &project,
            &name,
            None,
            |environment| app_vault.has_values(&project, environment),
            || app_vault.get_or_open(&app),
            DELETE_LOCKED,
        )?;
        dea_project::delete_environment(&project, &name)?;
        if let Some(vault) = vault {
            vault.remove_environment(&project, &name)?;
        }
        info_of(&session)
    })
    .await?
}

#[tauri::command]
pub async fn duplicate_environment(
    app: AppHandle,
    window: WebviewWindow,
    name: String,
) -> Result<ProjectInfo, IpcError> {
    let session = writable(&app, window.label())?;
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        dea_project::duplicate_environment(&project, &name)?;
        info_of(&session)
    })
    .await?
}

/// What uses the environment: project files, and other windows that have
/// it active.
#[tauri::command]
pub async fn environment_references(
    app: AppHandle,
    window: WebviewWindow,
    name: String,
) -> Result<Vec<EnvReference>, IpcError> {
    let session = writable(&app, window.label())?;
    let others = {
        let state = app.state::<AppState>();
        let all = state.sessions().windows_using(&session.scope, &name);
        let this = u32::from(session.active_environment.as_deref() == Some(name.as_str()));
        all - this
    };
    spawn_blocking(move || {
        let project = load_project(&session.root)?.project;
        let mut refs = dea_project::environment_references(&project, &name);
        if others > 0 {
            refs.push(EnvReference {
                label: if others == 1 {
                    "Open in 1 other window".into()
                } else {
                    format!("Open in {others} other windows")
                },
                count: others,
            });
        }
        Ok(refs)
    })
    .await?
}

#[tauri::command]
pub fn set_active_environment(app: AppHandle, window: WebviewWindow, name: Option<String>) {
    app.state::<AppState>()
        .sessions()
        .set_environment(window.label(), name);
}

#[tauri::command]
pub async fn clear_local_data(app: AppHandle, window: WebviewWindow) -> Result<(), IpcError> {
    // Local data can be cleared even when the project folder is gone.
    let scope = app
        .state::<AppState>()
        .sessions()
        .get(window.label())
        .map(|s| s.scope.clone())
        .ok_or(crate::session::Refusal::NoProject)?;
    let store = app.state::<AppState>().store.clone();
    Ok(spawn_blocking(move || store.clear_project(&scope)).await??)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::vault::{AppVault, PromptWindows, UiPrompter, VaultPromptEvent};

    #[test]
    fn a_broken_dea_yaml_is_a_file_problem_not_a_missing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        dea_project::create_project(&root, "p").unwrap();
        fs::write(
            root.join("dea.yaml"),
            "<<<<<<< HEAD\nname: p\n=======\nname: q\n>>>>>>> other\n",
        )
        .unwrap();
        let session = Session {
            root,
            scope: "p".into(),
            slot: 1,
            active_environment: None,
            missing: false,
        };
        let info = info_of(&session).unwrap();
        assert!(!info.missing);
        assert_eq!(info.slot, 1);
        assert_eq!(info.errors.len(), 1, "{:?}", info.errors);
        assert_eq!(info.errors[0].path, "dea.yaml");
        assert!(info.errors[0].merge_conflict);
    }

    #[test]
    fn turning_production_off_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        // local, staging, and production (marked production).
        let project = dea_project::create_project(&dir.path().join("p"), "p").unwrap();
        let env = |name: &str, production: bool| Environment {
            name: name.into(),
            production,
            color: None,
            variables: Vec::new(),
        };

        assert!(production_turned_off(
            &project,
            Some("production"),
            &env("production", false)
        ));
        // Renaming it at the same time does not hide the change.
        assert!(production_turned_off(
            &project,
            Some("production"),
            &env("live", false)
        ));
        assert!(!production_turned_off(
            &project,
            Some("production"),
            &env("live", true)
        ));
        assert!(!production_turned_off(
            &project,
            Some("production"),
            &env("production", true)
        ));
        // Staging was never production; a new environment has nothing to
        // turn off.
        assert!(!production_turned_off(
            &project,
            Some("staging"),
            &env("staging", false)
        ));
        assert!(!production_turned_off(
            &project,
            None,
            &env("production", false)
        ));
    }

    #[test]
    fn renaming_production_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let project = dea_project::create_project(&dir.path().join("p"), "p").unwrap();
        let env = |name: &str, production: bool| Environment {
            name: name.into(),
            production,
            color: None,
            variables: Vec::new(),
        };

        assert!(renames_production(
            &project,
            Some("production"),
            &env("live", true)
        ));
        assert!(!renames_production(
            &project,
            Some("staging"),
            &env("qa", false)
        ));
        assert!(!renames_production(
            &project,
            Some("production"),
            &env("production", true)
        ));
        assert!(!renames_production(&project, None, &env("live", true)));
    }

    #[test]
    fn a_rename_is_a_new_name_for_an_environment_on_disk() {
        let env = |name: &str| Environment {
            name: name.into(),
            production: false,
            color: None,
            variables: Vec::new(),
        };
        assert_eq!(
            renamed(Some("production"), &env("prod")),
            Some(("production", "prod"))
        );
        assert_eq!(renamed(Some("staging"), &env("staging")), None);
        // A new environment frees no name.
        assert_eq!(renamed(None, &env("prod")), None);
    }

    /// Stands in for the Vault: locked or not, and how often activity was
    /// recorded.
    #[derive(Debug)]
    struct FakeVault {
        locked: bool,
        activity: AtomicU32,
    }

    impl FakeVault {
        fn new(locked: bool) -> Arc<Self> {
            Arc::new(FakeVault {
                locked,
                activity: AtomicU32::new(0),
            })
        }

        fn activity(&self) -> u32 {
            self.activity.load(Ordering::SeqCst)
        }
    }

    impl ValuesVault for FakeVault {
        fn locked(&self) -> bool {
            self.locked
        }

        fn record_activity(&self) {
            self.activity.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// No window can show a Vault password prompt.
    struct NoWindows;

    impl PromptWindows for NoWindows {
        fn focused(&self) -> Option<String> {
            None
        }

        fn show(&self, _window: &str, _prompt: VaultPromptEvent) -> bool {
            false
        }
    }

    /// The app's Vault in `app_data`, not opened.
    fn app_vault(app_data: &Path) -> AppVault {
        AppVault::new(app_data.to_path_buf(), UiPrompter::new(NoWindows))
    }

    /// Opening the Vault fails the test.
    fn never() -> Result<Arc<FakeVault>, IpcError> {
        panic!("the Vault was opened")
    }

    /// An index the Vault cannot read: it counts as holding values.
    fn damaged_index(app_data: &Path) {
        fs::create_dir_all(app_data.join("vault")).unwrap();
        fs::write(app_data.join("vault").join("index.json"), "{ not json").unwrap();
    }

    #[test]
    fn a_rename_or_delete_without_values_leaves_the_vault_closed() {
        let dir = tempfile::tempdir().unwrap();
        let project = dea_project::create_project(&dir.path().join("p"), "p").unwrap();
        // Nothing was ever stored on this device.
        let vault = app_vault(&dir.path().join("app-data"));
        let has_values = |name: &str| vault.has_values(&project, name);

        let kept = vault_for_values(
            &project,
            "staging",
            Some("qa"),
            has_values,
            never,
            RENAME_LOCKED,
        );
        assert!(kept.unwrap().is_none());
        let kept = vault_for_values(&project, "staging", None, has_values, never, DELETE_LOCKED);
        assert!(kept.unwrap().is_none());
        assert!(vault.existing().is_none());
    }

    #[test]
    fn an_environment_sharing_its_name_leaves_the_vault_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = dea_project::create_project(&dir.path().join("p"), "p").unwrap();
        // As after a git merge: two files define `staging`.
        let staging = project
            .environments
            .iter()
            .find(|e| e.name == "staging")
            .cloned()
            .unwrap();
        project.environments.push(staging);
        let app_data = dir.path().join("app-data");
        damaged_index(&app_data);
        let vault = app_vault(&app_data);
        let has_values = |name: &str| vault.has_values(&project, name);

        let kept = vault_for_values(
            &project,
            "staging",
            Some("qa"),
            has_values,
            never,
            RENAME_LOCKED,
        );
        assert!(kept.unwrap().is_none());
        let kept = vault_for_values(&project, "staging", None, has_values, never, DELETE_LOCKED);
        assert!(kept.unwrap().is_none());
    }

    #[test]
    fn values_to_move_or_remove_need_an_unlocked_vault() {
        let dir = tempfile::tempdir().unwrap();
        let project = dea_project::create_project(&dir.path().join("p"), "p").unwrap();
        let app_data = dir.path().join("app-data");
        damaged_index(&app_data);
        let vault = app_vault(&app_data);
        let has_values = |name: &str| vault.has_values(&project, name);

        let locked = FakeVault::new(true);
        let open_locked = || Ok(Arc::clone(&locked));
        let refused = vault_for_values(
            &project,
            "staging",
            Some("qa"),
            has_values,
            open_locked,
            RENAME_LOCKED,
        )
        .unwrap_err();
        assert_eq!(refused.kind, IpcErrorKind::VaultLocked);
        assert_eq!(
            refused.message,
            "unlock the Vault to rename an environment that has secrets"
        );
        let refused = vault_for_values(
            &project,
            "staging",
            None,
            has_values,
            open_locked,
            DELETE_LOCKED,
        )
        .unwrap_err();
        assert_eq!(
            refused.message,
            "unlock the Vault to delete an environment that has secrets"
        );
        assert_eq!(locked.activity(), 0);

        let unlocked = FakeVault::new(false);
        let open_unlocked = || Ok(Arc::clone(&unlocked));
        let opened = vault_for_values(
            &project,
            "staging",
            None,
            has_values,
            open_unlocked,
            DELETE_LOCKED,
        );
        assert!(opened.unwrap().is_some());
        // The idle timer starts again, so the Vault cannot lock between this
        // check and the move.
        assert_eq!(unlocked.activity(), 1);
    }

    #[test]
    fn deleting_production_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let project = dea_project::create_project(&dir.path().join("p"), "p").unwrap();
        assert!(deletes_production(&project, "production"));
        assert!(!deletes_production(&project, "staging"));
        // dea-project refuses to delete what is not there.
        assert!(!deletes_production(&project, "gone"));
    }
}
