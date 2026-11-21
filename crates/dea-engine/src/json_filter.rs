//! Filters a JSON body with a JSONPath expression (RFC 9535).

use serde_json::Value;
use serde_json_path::JsonPath;

use crate::pretty::reindent_json;

/// The nodes a path selected, as a pretty JSON array.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Filtered {
    /// `[]` when nothing matched.
    pub text: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FilterError {
    #[error("the body is not JSON")]
    NotJson,
    #[error("invalid JSONPath: {0}")]
    InvalidPath(String),
}

/// Runs `path` over `text` and returns the matches in document order.
pub fn filter_json(text: &str, path: &str) -> Result<Filtered, FilterError> {
    let doc: Value = serde_json::from_str(text).map_err(|_| FilterError::NotJson)?;
    let path = JsonPath::parse(path).map_err(|e| FilterError::InvalidPath(e.to_string()))?;
    let nodes = path.query(&doc).all();
    let count = nodes.len();
    // `Value`'s Display cannot fail; the result is valid JSON by construction.
    let array = Value::Array(nodes.into_iter().cloned().collect());
    Ok(Filtered {
        text: reindent_json(&array.to_string()),
        count,
    })
}
