//! Enforces the crate layers of the folder structure decision (K26): each
//! workspace crate belongs to one layer, and may only depend on the layers
//! its own layer allows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, MetadataCommand};
use serde::Deserialize;

/// The test helper crate, only allowed under `[dev-dependencies]`.
const TESTKIT: &str = "dea-testkit";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

/// A direct dependency declared in a workspace crate's `Cargo.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: DepKind,
}

/// Contents of `rules.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Layer name to the packages in it.
    pub layers: BTreeMap<String, Vec<String>>,
    /// Layer name to the layers it may depend on.
    pub allow: BTreeMap<String, Vec<String>>,
    /// External crate to the layers that may not depend on it.
    #[serde(default)]
    pub forbidden_external: BTreeMap<String, Vec<String>>,
}

impl Rules {
    fn layer_of(&self, package: &str) -> Option<&str> {
        self.layers
            .iter()
            .find(|(_, packages)| packages.iter().any(|p| p == package))
            .map(|(layer, _)| layer.as_str())
    }
}

#[derive(Debug, PartialEq)]
pub enum Violation {
    UnknownCrate(String),
    ForbiddenEdge { from: String, to: String },
    TestkitAsNormalDependency { from: String },
    ForbiddenExternal { from: String, dependency: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ArchError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid {path}: {message}")]
    InvalidRules { path: PathBuf, message: String },
    #[error("cannot read the workspace: {0}")]
    Metadata(#[from] cargo_metadata::Error),
}

pub fn load_rules(path: &Path) -> Result<Rules, ArchError> {
    let text = std::fs::read_to_string(path).map_err(|source| ArchError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|e| ArchError::InvalidRules {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Workspace members and every dependency they declare directly. Only the
/// manifests are read; nothing is resolved or downloaded.
pub fn workspace_graph(manifest: &Path) -> Result<(Vec<String>, Vec<Edge>), ArchError> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .exec()?;
    let mut members = Vec::new();
    let mut edges = Vec::new();
    for package in metadata.workspace_packages() {
        let from = package.name.to_string();
        for dep in &package.dependencies {
            let kind = match dep.kind {
                DependencyKind::Development => DepKind::Dev,
                DependencyKind::Build => DepKind::Build,
                _ => DepKind::Normal,
            };
            edges.push(Edge {
                from: from.clone(),
                to: dep.name.clone(),
                kind,
            });
        }
        members.push(from);
    }
    members.sort();
    Ok((members, edges))
}

/// Checks every member has a layer and every edge follows the rules:
/// `dea-testkit` only as a dev-dependency, other dev-dependencies unchecked,
/// normal and build dependencies on members only towards allowed layers,
/// and no forbidden external crate in the listed layers.
pub fn check(members: &[String], edges: &[Edge], rules: &Rules) -> Vec<Violation> {
    let mut out = Vec::new();
    for member in members {
        if rules.layer_of(member).is_none() {
            out.push(Violation::UnknownCrate(member.clone()));
        }
    }

    for edge in edges {
        let Some(from_layer) = rules.layer_of(&edge.from) else {
            continue; // Already reported as unknown.
        };
        if edge.to == TESTKIT {
            if edge.kind != DepKind::Dev {
                out.push(Violation::TestkitAsNormalDependency {
                    from: edge.from.clone(),
                });
            }
            continue;
        }
        if edge.kind == DepKind::Dev {
            continue;
        }

        if members.contains(&edge.to) {
            let allowed = rules.layer_of(&edge.to).is_some_and(|to_layer| {
                rules
                    .allow
                    .get(from_layer)
                    .is_some_and(|allowed| allowed.iter().any(|l| l == to_layer))
            });
            if !allowed {
                out.push(Violation::ForbiddenEdge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                });
            }
        } else if rules
            .forbidden_external
            .get(&edge.to)
            .is_some_and(|layers| layers.iter().any(|l| l == from_layer))
        {
            out.push(Violation::ForbiddenExternal {
                from: edge.from.clone(),
                dependency: edge.to.clone(),
            });
        }
    }
    out
}

/// One line describing a violation, naming the layers involved.
pub fn explain(violation: &Violation, rules: &Rules) -> String {
    let layer = |p: &str| rules.layer_of(p).unwrap_or("an unknown layer").to_string();
    match violation {
        Violation::UnknownCrate(name) => {
            format!("{name}: not assigned to a layer in tools/arch-check/rules.toml")
        }
        Violation::ForbiddenEdge { from, to } => {
            format!(
                "{from} -> {to}: {} may not depend on {}",
                layer(from),
                layer(to)
            )
        }
        Violation::TestkitAsNormalDependency { from } => {
            format!("{from} -> {TESTKIT}: only allowed under [dev-dependencies]")
        }
        Violation::ForbiddenExternal { from, dependency } => {
            format!(
                "{from} -> {dependency}: {} crates may not use {dependency}",
                layer(from)
            )
        }
    }
}
