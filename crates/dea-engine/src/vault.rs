//! The one way to read a secret: checks the lock, asks for production
//! authentication, and reports status changes (F1-FON-08, 09, 10).
//!
//! Everything the app needs from the Vault is here, so the app never
//! depends on `dea-vault` itself.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use dea_project::{Project, SecretKind, SecretLookup, SecretLookupError};
use dea_vault::{
    AuthError, AuthMethod, Authenticator, KdfParams, LockState, OsEvent, OsEventsWatch, SecretData,
    SecretStatus, SecretStore, VaultBackend, VaultPassword, watch_os_events,
};
use serde::Serialize;

use crate::Clock;
pub use crate::clipboard::{CLIPBOARD_CLEAR_AFTER, SystemClipboard};
use crate::clipboard::{Clipboard, copy_and_clear_later};
pub use crate::error::UseError;
pub use crate::prompt::{PasswordPrompt, PromptProblem, Prompter};
pub use crate::vault_app::{
    SaveSecretError, SecretInput, SecretRow, UnlockMethod, app_vault_has_values, open_app_vault,
    save_secret, unlock_method,
};
pub use dea_vault::{MIN_PASSWORD_CHARS, Policy, SecretKey, VaultError};
pub use secrecy::SecretString;

/// How often the idle timer is checked. Below one second, so the status
/// changes within a second of the Vault locking (F1-FON-09 #6).
pub const TICK: Duration = Duration::from_millis(500);

/// File name of the Linux Vault password hash inside the Vault folder.
pub const PASSWORD_FILE: &str = "password.phc";

/// Whether the Vault is locked; sent to every window when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct VaultStatus {
    pub locked: bool,
}

/// The production dialog text: `<action> using `a`, `b``, or only the
/// action when no secret is involved, as when turning production off.
pub fn production_reason(action: &str, secrets: &[&str]) -> String {
    if secrets.is_empty() {
        return action.to_string();
    }
    let names: Vec<String> = secrets.iter().map(|s| format!("`{s}`")).collect();
    format!("{action} using {}", names.join(", "))
}

pub struct VaultService {
    backend: VaultBackend,
    store: SecretStore,
    password: VaultPassword,
    authenticator: Box<dyn Authenticator>,
    prompter: Arc<dyn Prompter>,
    policy: Mutex<Policy>,
    kdf: KdfParams,
    clock: Arc<dyn Clock>,
    state: Mutex<LockState>,
    /// Held while a dialog is open, so only one shows at a time.
    auth_gate: Mutex<()>,
    on_status: Box<dyn Fn(VaultStatus) + Send + Sync>,
    /// The timer and OS event watcher the service keeps running itself,
    /// see [`VaultService::run_in_background`].
    background: Mutex<Option<VaultBackground>>,
}

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl VaultService {
    /// `dir` is the Vault folder in the app data: it holds `index.json`,
    /// `password.phc` and, for the file backend, `vault.enc`. The Vault
    /// starts locked.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dir: &Path,
        backend: VaultBackend,
        authenticator: Box<dyn Authenticator>,
        prompter: Arc<dyn Prompter>,
        policy: Policy,
        kdf: KdfParams,
        clock: Arc<dyn Clock>,
        on_status: Box<dyn Fn(VaultStatus) + Send + Sync>,
    ) -> Result<Arc<Self>, VaultError> {
        Ok(Arc::new(VaultService {
            store: SecretStore::open(dir, backend.as_backend())?,
            password: VaultPassword::open(&dir.join(PASSWORD_FILE)),
            backend,
            authenticator,
            prompter,
            policy: Mutex::new(policy),
            kdf,
            clock,
            state: Mutex::new(LockState::new()),
            auth_gate: Mutex::new(()),
            on_status,
            background: Mutex::new(None),
        }))
    }

    pub fn status(&self) -> VaultStatus {
        VaultStatus {
            locked: !guard(&self.state).is_unlocked(),
        }
    }

    fn report(&self, locked: bool) {
        (self.on_status)(VaultStatus { locked });
    }

    pub fn set_policy(&self, policy: Policy) {
        *guard(&self.policy) = policy;
    }

    pub fn record_activity(&self) {
        guard(&self.state).record_activity(self.clock.now());
    }

    /// Locks now: by hand, on idle, or on an OS event. Forgets the file
    /// backend's key and closes the production window.
    pub fn lock(&self) {
        let was_unlocked = {
            let mut state = guard(&self.state);
            let was = state.is_unlocked();
            state.lock();
            was
        };
        if let VaultBackend::File(file) = &self.backend {
            file.lock();
        }
        if was_unlocked {
            self.report(true);
        }
    }

    /// Locks after the idle time or a resume from sleep. Called every
    /// [`TICK`] by [`VaultService::start_background`].
    pub fn tick(&self) {
        let policy = *guard(&self.policy);
        let locked = guard(&self.state).tick(self.clock.now(), self.clock.wall(), &policy);
        if locked {
            if let VaultBackend::File(file) = &self.backend {
                file.lock();
            }
            self.report(true);
        }
    }

    pub fn handle_os_event(&self, _event: OsEvent) {
        self.lock();
    }

    /// Ticks and watches for screen lock and sleep until the returned value
    /// is dropped.
    pub fn start_background(self: &Arc<Self>) -> Result<VaultBackground, VaultError> {
        let stop = Arc::new(AtomicBool::new(false));
        let weak: Weak<Self> = Arc::downgrade(self);
        let ticker_stop = Arc::clone(&stop);
        let ticker_weak = weak.clone();
        std::thread::Builder::new()
            .name("dea-vault-ticker".into())
            .spawn(move || {
                while !ticker_stop.load(Ordering::SeqCst) {
                    std::thread::sleep(TICK);
                    match ticker_weak.upgrade() {
                        Some(vault) => vault.tick(),
                        None => break,
                    }
                }
            })
            .map_err(|e| VaultError::Backend(format!("cannot start the Vault timer: {e}")))?;
        let watch = watch_os_events(move |event| {
            if let Some(vault) = weak.upgrade() {
                vault.handle_os_event(event);
            }
        })?;
        Ok(VaultBackground {
            stop,
            _watch: watch,
        })
    }

    /// Like [`VaultService::start_background`], but the service keeps the
    /// timer and watcher until it is dropped itself.
    pub(crate) fn run_in_background(self: &Arc<Self>) -> Result<(), VaultError> {
        let background = self.start_background()?;
        *guard(&self.background) = Some(background);
        Ok(())
    }

    /// Unlocks with the OS dialog, or on Linux the Vault password.
    pub fn unlock(&self, reason: &str) -> Result<(), UseError> {
        let _gate = guard(&self.auth_gate);
        if guard(&self.state).is_unlocked() {
            return Ok(());
        }
        self.authenticate(reason)?;
        guard(&self.state).unlock(self.clock.now(), self.clock.wall());
        self.report(false);
        Ok(())
    }

    /// Makes production secrets usable for the production window, asking
    /// for authentication if the window is closed. Unlocks too, with the
    /// same single dialog. Call only for production environments.
    pub fn authorize_production(&self, action: &str, secrets: &[&str]) -> Result<(), UseError> {
        if guard(&self.state).production_open(self.clock.now()) {
            return Ok(());
        }
        let _gate = guard(&self.auth_gate);
        // Another caller may have authenticated while this one waited.
        if guard(&self.state).production_open(self.clock.now()) {
            return Ok(());
        }
        self.authenticate(&production_reason(action, secrets))?;
        let policy = *guard(&self.policy);
        let unlocked_now = {
            let mut state = guard(&self.state);
            let was_locked = !state.is_unlocked();
            if was_locked {
                state.unlock(self.clock.now(), self.clock.wall());
            }
            state.open_production(self.clock.now(), &policy);
            was_locked
        };
        if unlocked_now {
            self.report(false);
        }
        Ok(())
    }

    /// Whether production secrets are usable now without asking again.
    pub fn production_open(&self) -> bool {
        guard(&self.state).production_open(self.clock.now())
    }

    /// One authentication, native or by Vault password. The caller holds
    /// `auth_gate`.
    fn authenticate(&self, reason: &str) -> Result<(), UseError> {
        if let Err(wait) = guard(&self.state).attempt_allowed(self.clock.now()) {
            return Err(UseError::RetryAfter(wait));
        }
        if self.authenticator.method() == AuthMethod::VaultPassword {
            return self.vault_password(reason);
        }
        match self.authenticator.authenticate(reason) {
            Ok(()) => {
                guard(&self.state).record_success();
                Ok(())
            }
            Err(AuthError::Cancelled) => Err(UseError::Cancelled),
            Err(AuthError::Failed) => {
                guard(&self.state).record_failure(self.clock.now());
                Err(UseError::AuthFailed)
            }
            Err(AuthError::Unavailable) => Err(UseError::Vault(VaultError::Backend(
                "this device cannot authenticate".into(),
            ))),
            Err(AuthError::Other(e)) => Err(UseError::Vault(VaultError::Backend(e))),
        }
    }

    /// The Linux flow: create the Vault password the first time (K19),
    /// otherwise ask until it is right, cancelled, or held back.
    fn vault_password(&self, reason: &str) -> Result<(), UseError> {
        let mut problem = None;
        loop {
            let prompt = |problem| PasswordPrompt {
                reason: reason.to_string(),
                problem,
            };
            // Read first: a guard in the `if let` would stay locked for the
            // whole block, and with it every status check while the prompt
            // is open.
            let allowed = guard(&self.state).attempt_allowed(self.clock.now());
            if let Err(wait) = allowed {
                let _ = self
                    .prompter
                    .vault_password(&prompt(Some(PromptProblem::RetryAfter(wait))));
                return Err(UseError::RetryAfter(wait));
            }
            if !self.password.is_set() {
                let answer = self
                    .prompter
                    .new_vault_password(&prompt(problem.take()))
                    .ok_or(UseError::Cancelled)?;
                match self.password.set(&answer, self.kdf) {
                    Err(VaultError::PasswordTooShort { min }) => {
                        problem = Some(PromptProblem::TooShort { min });
                        continue;
                    }
                    other => other?,
                }
                self.open_file(&answer)?;
                guard(&self.state).record_success();
                return Ok(());
            }
            let answer = self
                .prompter
                .vault_password(&prompt(problem.take()))
                .ok_or(UseError::Cancelled)?;
            if self.password.verify(&answer)? {
                self.open_file(&answer)?;
                guard(&self.state).record_success();
                return Ok(());
            }
            guard(&self.state).record_failure(self.clock.now());
            problem = Some(PromptProblem::WrongPassword);
        }
    }

    /// For the encrypted file backend: unlocks it, or creates it the first
    /// time, with the Vault password.
    fn open_file(&self, password: &SecretString) -> Result<(), VaultError> {
        match &self.backend {
            VaultBackend::File(file) if file.exists() => file.unlock(password),
            VaultBackend::File(file) => file.create(password),
            _ => Ok(()),
        }
    }

    fn require_unlocked(&self) -> Result<(), UseError> {
        if guard(&self.state).is_unlocked() {
            Ok(())
        } else {
            Err(UseError::Locked)
        }
    }

    pub fn set_secret(
        &self,
        key: &SecretKey,
        kind: SecretKind,
        data: &SecretData,
    ) -> Result<(), UseError> {
        self.require_unlocked()?;
        self.store.set(key, kind, data)?;
        self.record_activity();
        Ok(())
    }

    pub fn delete_secret(&self, key: &SecretKey) -> Result<(), UseError> {
        self.require_unlocked()?;
        self.store.delete(key)?;
        self.record_activity();
        Ok(())
    }

    /// Whether `environment` of `project` has a value on this device. Reads
    /// only the index, so it works while locked.
    pub fn has_values(&self, project: &Project, environment: &str) -> bool {
        self.store.has_values(&project.vault_scope(), environment)
    }

    /// Moves the values of environment `from` of `project` to `to`, for an
    /// environment renamed on disk, so none stay behind under the freed
    /// name. `to` ends with exactly what `from` had. `project` is the one
    /// before the rename: when it has more than one environment called
    /// `from`, the others still use the values, and nothing moves (see
    /// [`values_involved`]). Nothing to move needs no unlock.
    ///
    /// # Errors
    ///
    /// [`UseError::Locked`] when `from` or `to` has values and the Vault is
    /// locked; nothing is moved then. [`UseError::Vault`] when the store
    /// fails.
    pub fn move_environment(
        &self,
        project: &Project,
        from: &str,
        to: &str,
    ) -> Result<(), UseError> {
        let scope = project.vault_scope();
        if !values_involved(project, from, Some(to), |name| {
            self.store.has_values(&scope, name)
        }) {
            return Ok(());
        }
        self.require_unlocked()?;
        self.store.move_environment(&scope, from, to)?;
        self.record_activity();
        Ok(())
    }

    /// Removes the values of `environment` of `project`, for an environment
    /// deleted on disk, so none stay behind under the freed name. `project`
    /// is the one before the delete: when it has more than one environment
    /// of that name, the others still use the values, and nothing is removed
    /// (see [`values_involved`]). Nothing to remove needs no unlock.
    ///
    /// # Errors
    ///
    /// [`UseError::Locked`] when the environment has values and the Vault
    /// is locked; nothing is removed then. [`UseError::Vault`] when the
    /// store fails.
    pub fn remove_environment(&self, project: &Project, environment: &str) -> Result<(), UseError> {
        let scope = project.vault_scope();
        if !values_involved(project, environment, None, |name| {
            self.store.has_values(&scope, name)
        }) {
            return Ok(());
        }
        self.require_unlocked()?;
        self.store.delete_environment(&scope, environment)?;
        self.record_activity();
        Ok(())
    }

    /// Every secret the project needs, per environment. Works while locked.
    pub fn statuses(&self, project: &Project) -> Vec<SecretStatus> {
        let environments: Vec<String> = project
            .environments
            .iter()
            .map(|e| e.name.clone())
            .collect();
        self.store
            .statuses(&project.vault_scope(), &project.vault, &environments)
    }

    /// The value of secret `name` in `environment` of `project`, or `None`
    /// if this device has none. Whether the environment is production comes
    /// from the project itself; an environment the project does not have is
    /// treated as production.
    pub fn secret(
        &self,
        project: &Project,
        environment: &str,
        name: &str,
    ) -> Result<Option<SecretData>, UseError> {
        self.read(
            &project.vault_scope(),
            environment,
            name,
            is_production(project, environment),
        )
    }

    fn read(
        &self,
        scope: &str,
        environment: &str,
        name: &str,
        production: bool,
    ) -> Result<Option<SecretData>, UseError> {
        {
            let state = guard(&self.state);
            if !state.is_unlocked() {
                return Err(UseError::Locked);
            }
            if production && !state.production_open(self.clock.now()) {
                return Err(UseError::ReauthRequired);
            }
        }
        let key = SecretKey {
            scope: scope.to_string(),
            environment: environment.to_string(),
            name: name.to_string(),
        };
        let value = self.store.get(&key)?;
        self.record_activity();
        Ok(value)
    }

    /// Copies the value and clears the clipboard after `clear_after`
    /// ([`crate::CLIPBOARD_CLEAR_AFTER`] in the app) unless the person
    /// copied something else. Same checks as [`VaultService::secret`].
    pub fn copy_secret(
        &self,
        project: &Project,
        environment: &str,
        name: &str,
        clipboard: Arc<dyn Clipboard>,
        clear_after: Duration,
    ) -> Result<(), UseError> {
        let value = self
            .secret(project, environment, name)?
            .ok_or(UseError::Missing)?;
        copy_and_clear_later(clipboard, value.value_for_use(), clear_after).map_err(|e| {
            UseError::Vault(VaultError::Backend(format!(
                "cannot use the clipboard: {e}"
            )))
        })
    }

    /// Secret values for variable resolution in `project`. Each lookup
    /// checks whether its environment is production.
    pub fn lookup<'a>(&'a self, project: &'a Project) -> VaultLookup<'a> {
        VaultLookup {
            vault: self,
            project,
            scope: project.vault_scope(),
        }
    }
}

/// Whether `environment` of `project` holds production secrets: any
/// environment of that name is marked production, as two files can share a
/// name after a merge. Unknown environments count as production, so a
/// mistake asks once too often rather than never.
#[must_use]
pub fn is_production(project: &Project, environment: &str) -> bool {
    let mut named = project
        .environments
        .iter()
        .filter(|e| e.name == environment)
        .peekable();
    named.peek().is_none() || named.any(|e| e.production)
}

/// Whether renaming `environment` of `project` to `to`, or deleting it
/// when `to` is `None`, involves Vault values; `has_values` tells whether a
/// name has values on this device. `project` is the one before the change.
///
/// Only when exactly one environment has the name: two files can share one
/// after a merge, and the values are then used by both, so moving or
/// removing them with one would take them from the other. A rename also
/// involves values left under `to`, which it removes.
#[must_use]
pub fn values_involved(
    project: &Project,
    environment: &str,
    to: Option<&str>,
    has_values: impl Fn(&str) -> bool,
) -> bool {
    let named = project
        .environments
        .iter()
        .filter(|e| e.name == environment)
        .count();
    named == 1 && (has_values(environment) || to.is_some_and(&has_values))
}

/// Stops the timer and the OS event watcher when dropped.
pub struct VaultBackground {
    stop: Arc<AtomicBool>,
    _watch: OsEventsWatch,
}

impl Drop for VaultBackground {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub struct VaultLookup<'a> {
    vault: &'a VaultService,
    project: &'a Project,
    scope: String,
}

impl SecretLookup for VaultLookup<'_> {
    fn lookup(&self, environment: &str, name: &str) -> Result<Option<String>, SecretLookupError> {
        let production = is_production(self.project, environment);
        match self.vault.read(&self.scope, environment, name, production) {
            Ok(value) => Ok(value.map(|v| v.value_for_use().to_string())),
            Err(UseError::Locked) => Err(SecretLookupError::Locked),
            Err(UseError::ReauthRequired) => Err(SecretLookupError::ReauthRequired),
            Err(other) => Err(SecretLookupError::Other(other.to_string())),
        }
    }
}
