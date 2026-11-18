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
    /// `%`, `/` and `#` are percent-encoded inside each part, so no two keys
    /// share an account, and `#` stays free for the keychain's chunk entries.
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
    for c in part.chars() {
        match c {
            '%' => out.push_str("%25"),
            '/' => out.push_str("%2F"),
            '#' => out.push_str("%23"),
            _ => out.push(c),
        }
    }
    out
}
