//! JSON Schemas of the project files, generated from the Rust types so they
//! always match what Dea reads. Published in `crates/dea-project/schemas/`.

use schemars::JsonSchema;
use serde_json::Value;

use crate::{Environment, Manifest, VaultManifest};

/// Every schema with the file name it is published under.
pub fn json_schemas() -> Vec<(&'static str, Value)> {
    vec![
        ("manifest.schema.json", schema::<Manifest>()),
        ("environment.schema.json", schema::<Environment>()),
        ("vault.schema.json", schema::<VaultManifest>()),
    ]
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("a schema is always valid JSON")
}
