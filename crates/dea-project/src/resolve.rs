//! Resolves `{{name}}`, `{{vault:name}}` and `{{$dynamic}}` placeholders.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use time::OffsetDateTime;

use crate::Variable;

static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap());

/// How many variables may refer to each other in a chain.
const MAX_DEPTH: usize = 10;

/// Largest resolved text. A few kilobytes of crafted variables that refer
/// to each other many times per level would otherwise expand without bound.
pub const MAX_RESOLVED_BYTES: usize = 16 * 1024 * 1024;

/// Where a variable is defined, most specific first. A variable in a more
/// specific scope hides one with the same name in a less specific scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeKind {
    Request,
    Folder,
    Collection,
    Environment,
    Global,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    pub kind: ScopeKind,
    pub label: String,
    pub variables: Vec<Variable>,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum SecretLookupError {
    #[error("the Vault is locked")]
    Locked,
    #[error("this production secret needs authentication again")]
    ReauthRequired,
    #[error("{0}")]
    Other(String),
}

/// Reads secret values. Implemented by the Vault.
pub trait SecretLookup {
    fn lookup(&self, environment: &str, name: &str) -> Result<Option<String>, SecretLookupError>;
}

/// Values for the Postman dynamic variables. Replaceable in tests.
pub trait DynamicSource {
    fn guid(&mut self) -> String;
    fn unix_timestamp(&mut self) -> i64;
    fn iso_timestamp(&mut self) -> String;
    fn random_int(&mut self) -> u32;
}

pub struct SystemDynamicSource;

impl DynamicSource for SystemDynamicSource {
    fn guid(&mut self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn unix_timestamp(&mut self) -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    /// UTC with milliseconds, like Postman: `2025-11-17T09:30:00.123Z`.
    fn iso_timestamp(&mut self) -> String {
        let t = OffsetDateTime::now_utc();
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second(),
            t.millisecond()
        )
    }

    fn random_int(&mut self) -> u32 {
        rand::random_range(0..=1000)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueSource {
    Scope { kind: ScopeKind, label: String },
    Vault { environment: String, name: String },
    Dynamic(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsedVariable {
    pub name: String,
    pub source: ValueSource,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    /// The text to send.
    pub text: String,
    /// The same text with every secret value replaced by `[secret:<name>]`,
    /// for history, logs and display.
    pub redacted_text: String,
    /// Placeholders with no value, left in the text as written.
    pub unresolved: Vec<String>,
    pub used: Vec<UsedVariable>,
}

pub struct ResolveContext<'a> {
    pub scopes: &'a [Scope],
    pub environment: &'a str,
    pub secrets: &'a dyn SecretLookup,
    pub dynamic: &'a mut dyn DynamicSource,
}

#[derive(thiserror::Error, Debug)]
pub enum ResolveError {
    #[error("variable {0} refers to itself")]
    Cycle(String),
    #[error("variable {0} is nested more than 10 levels deep")]
    TooDeep(String),
    #[error("the resolved text is larger than {0} bytes")]
    TooLarge(usize),
    #[error("cannot read secret {name}: {error}")]
    Secret {
        name: String,
        error: SecretLookupError,
    },
}

/// Replaces every placeholder in `template`. Unknown placeholders are left
/// as written and listed in `unresolved`; only cycles, chains deeper than 10
/// and Vault failures are errors.
pub fn resolve(template: &str, ctx: &mut ResolveContext) -> Result<Resolution, ResolveError> {
    let mut scopes: Vec<&Scope> = ctx.scopes.iter().collect();
    // Precedence comes from the kind; within a kind, earlier scopes win.
    scopes.sort_by_key(|s| s.kind);
    let mut resolver = Resolver {
        ctx,
        scopes,
        unresolved: Vec::new(),
        used: Vec::new(),
        stack: Vec::new(),
        variables: HashMap::new(),
        secrets: HashMap::new(),
    };
    let expanded = resolver.expand(template)?;
    Ok(Resolution {
        text: expanded.text,
        redacted_text: expanded.redacted,
        unresolved: resolver.unresolved,
        used: resolver.used,
    })
}

#[derive(Clone)]
struct Expanded {
    text: String,
    redacted: String,
    secret: bool,
}

struct Resolver<'c, 'a> {
    ctx: &'c mut ResolveContext<'a>,
    scopes: Vec<&'a Scope>,
    unresolved: Vec<String>,
    used: Vec<UsedVariable>,
    /// Variables being resolved, to detect cycles.
    stack: Vec<String>,
    /// Each variable is expanded and each secret read once per call, so
    /// repeated references cost nothing and never ask the Vault twice.
    variables: HashMap<String, Expanded>,
    secrets: HashMap<String, Option<Expanded>>,
}

impl Resolver<'_, '_> {
    fn expand(&mut self, template: &str) -> Result<Expanded, ResolveError> {
        let mut out = Expanded {
            text: String::with_capacity(template.len()),
            redacted: String::with_capacity(template.len()),
            secret: false,
        };
        let mut last = 0;
        for caps in PLACEHOLDER.captures_iter(template) {
            let whole = caps.get(0).expect("group 0 always matches");
            let literal = &template[last..whole.start()];
            out.text.push_str(literal);
            out.redacted.push_str(literal);
            let name = caps[1].trim();
            match self.placeholder(name)? {
                Some(value) => {
                    if out.text.len() + value.text.len() > MAX_RESOLVED_BYTES {
                        return Err(ResolveError::TooLarge(MAX_RESOLVED_BYTES));
                    }
                    out.text.push_str(&value.text);
                    out.redacted.push_str(&value.redacted);
                    out.secret |= value.secret;
                }
                None => {
                    out.text.push_str(whole.as_str());
                    out.redacted.push_str(whole.as_str());
                    if !self.unresolved.iter().any(|n| n == name) {
                        self.unresolved.push(name.to_string());
                    }
                }
            }
            last = whole.end();
        }
        out.text.push_str(&template[last..]);
        out.redacted.push_str(&template[last..]);
        Ok(out)
    }

    fn placeholder(&mut self, name: &str) -> Result<Option<Expanded>, ResolveError> {
        if let Some(secret) = name.strip_prefix("vault:") {
            return self.secret(secret.trim());
        }
        if name.starts_with('$') {
            return Ok(self.dynamic(name));
        }
        self.variable(name)
    }

    fn secret(&mut self, name: &str) -> Result<Option<Expanded>, ResolveError> {
        if name.is_empty() {
            return Ok(None);
        }
        if let Some(cached) = self.secrets.get(name) {
            return Ok(cached.clone());
        }
        let value = self.read_secret(name)?;
        self.secrets.insert(name.to_string(), value.clone());
        Ok(value)
    }

    fn read_secret(&mut self, name: &str) -> Result<Option<Expanded>, ResolveError> {
        let environment = self.ctx.environment;
        match self.ctx.secrets.lookup(environment, name) {
            // Secret values are used as they are, never expanded.
            Ok(Some(value)) => {
                self.record(
                    None,
                    UsedVariable {
                        name: name.to_string(),
                        source: ValueSource::Vault {
                            environment: environment.to_string(),
                            name: name.to_string(),
                        },
                        secret: true,
                    },
                );
                Ok(Some(Expanded {
                    text: value,
                    redacted: format!("[secret:{name}]"),
                    secret: true,
                }))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(ResolveError::Secret {
                name: name.to_string(),
                error,
            }),
        }
    }

    fn dynamic(&mut self, name: &str) -> Option<Expanded> {
        let source = &mut *self.ctx.dynamic;
        let value = match name {
            "$guid" => source.guid(),
            "$timestamp" => source.unix_timestamp().to_string(),
            "$isoTimestamp" => source.iso_timestamp(),
            "$randomInt" => source.random_int().to_string(),
            _ => return None,
        };
        self.record(
            None,
            UsedVariable {
                name: name.to_string(),
                source: ValueSource::Dynamic(name.to_string()),
                secret: false,
            },
        );
        Some(Expanded {
            redacted: value.clone(),
            text: value,
            secret: false,
        })
    }

    fn variable(&mut self, name: &str) -> Result<Option<Expanded>, ResolveError> {
        let found = self.scopes.iter().find_map(|scope| {
            scope
                .variables
                .iter()
                .find(|v| v.name == name)
                .map(|v| (*scope, v))
        });
        let Some((scope, variable)) = found else {
            return Ok(None);
        };
        if let Some(cached) = self.variables.get(name) {
            return Ok(Some(cached.clone()));
        }
        if self.stack.iter().any(|n| n == name) {
            return Err(ResolveError::Cycle(name.to_string()));
        }
        if self.stack.len() >= MAX_DEPTH {
            return Err(ResolveError::TooDeep(name.to_string()));
        }

        // Listed before the variables its value uses.
        let position = self.used.len();
        self.stack.push(name.to_string());
        let value = self.expand(&variable.value)?;
        self.stack.pop();
        self.variables.insert(name.to_string(), value.clone());
        self.record(
            Some(position),
            UsedVariable {
                name: name.to_string(),
                source: ValueSource::Scope {
                    kind: scope.kind,
                    label: scope.label.clone(),
                },
                secret: value.secret,
            },
        );
        Ok(Some(value))
    }

    /// Adds `used` once, at `position` or at the end, skipping duplicates.
    fn record(&mut self, position: Option<usize>, used: UsedVariable) {
        if self.used.contains(&used) {
            return;
        }
        match position {
            Some(position) => self.used.insert(position.min(self.used.len()), used),
            None => self.used.push(used),
        }
    }
}
