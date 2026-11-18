//! Questions only the person can answer. The desktop app shows them as
//! dialogs (Rencana 3); a CLI would ask in the terminal.

use std::time::Duration;

use secrecy::SecretString;

/// What to show in a Vault password prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordPrompt {
    /// The action waiting on the answer, for example
    /// "Send POST /v1/shipments to production using `prod_access_token`".
    pub reason: String,
    /// Why the previous answer was not accepted, if it was not.
    pub problem: Option<PromptProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptProblem {
    WrongPassword,
    TooShort {
        min: usize,
    },
    /// Too many wrong answers: the prompt can only be closed for now.
    RetryAfter(Duration),
}

pub trait Prompter: Send + Sync {
    /// Asks for the Vault password (Linux). `None` when the person cancels.
    fn vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString>;
    /// Asks the person to choose a Vault password, the first time one is
    /// needed (K19). `None` when the person cancels.
    fn new_vault_password(&self, prompt: &PasswordPrompt) -> Option<SecretString>;
}
