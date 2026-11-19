//! The one error shape commands reject with, `{ kind, message }`.

use dea_engine::store::StoreError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct IpcError {
    pub kind: String,
    pub message: String,
}

impl IpcError {
    pub fn new(kind: &str, message: impl ToString) -> Self {
        Self {
            kind: kind.into(),
            message: message.to_string(),
        }
    }
}

impl From<StoreError> for IpcError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Invalid(_) => Self::new("invalid", e),
            StoreError::Io { .. } => Self::new("io", e),
        }
    }
}

impl From<tauri::Error> for IpcError {
    fn from(e: tauri::Error) -> Self {
        Self::new("internal", e)
    }
}
