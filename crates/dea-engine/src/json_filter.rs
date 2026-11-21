//! Filters a JSON body with a JSONPath expression (RFC 9535).

use serde_json::Value;
use serde_json_path::JsonPath;

use crate::pretty::pretty_json;

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
    let json = serde_json::to_string(&nodes).map_err(|_| FilterError::NotJson)?;
    let text = pretty_json(&json).unwrap_or_else(|| "[]".to_owned());
    Ok(Filtered { text, count })
}
