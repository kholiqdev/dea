//! Puts the Dea feature crates together for the desktop app and, later, the
//! CLI. Owns the gates that ask the person something: Vault unlock and
//! production authentication.

mod clock;
mod error;
mod prompt;
mod vault;

pub use clock::{Clock, SystemClock};
pub use error::UseError;
pub use prompt::{PasswordPrompt, PromptProblem, Prompter};
pub use vault::{
    PASSWORD_FILE, TICK, VaultBackground, VaultLookup, VaultService, VaultStatus, production_reason,
};
