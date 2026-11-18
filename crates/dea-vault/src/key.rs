/// Where one secret value lives: a project, one of its environments, and
/// the secret's name. The same name has a separate value per environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey {
    /// `Project::vault_scope()` of the project.
    pub scope: String,
    pub environment: String,
    pub name: String,
}

impl SecretKey {
    /// The account name in the secret store: `<scope>/<environment>/<name>`.
    /// Inside each part, every byte other than `a-z`, `0-9`, `.`, `_` and
    /// `-` is percent-encoded with upper-case hex. So no two keys share an
    /// account, not even on Windows, which compares names without regard to
    /// case, and `#` stays free for the keychain's chunk entries.
    pub fn account(&self) -> String {
        format!(
            "{}/{}/{}",
            encode(&self.scope),
            encode(&self.environment),
            encode(&self.name)
        )
    }
}

fn encode(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    for byte in part.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => out.push(char::from(byte)),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
