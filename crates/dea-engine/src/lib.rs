//! Puts the Dea feature crates together for the desktop app and, later, the
//! CLI. Owns the gates that ask the person something: Vault unlock,
//! production authentication and the production send confirmation. Keeps
//! settings, UI state and cookies.

mod clipboard;
mod clock;
mod error;
mod inspection;
pub mod projects;
mod prompt;
pub mod requests;
pub mod store;
pub mod vault;
mod vault_app;

pub use clipboard::{CLIPBOARD_CLEAR_AFTER, Clipboard, SystemClipboard};
pub use clock::{Clock, SystemClock};
pub use error::UseError;
pub use inspection::{AutomaticHeader, Inspection, OptionsInfo, VariableInfo};
pub use prompt::{PasswordPrompt, PromptProblem, Prompter, SendAnswer, SendConfirmation};
pub use requests::{PreparedRequest, RequestService, SendError, Sent};
pub use vault::{
    PASSWORD_FILE, TICK, VaultBackground, VaultLookup, VaultService, VaultStatus, production_reason,
};
