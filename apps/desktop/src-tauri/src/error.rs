//! The one error shape commands reject with, `{ kind, message }`.

use std::error::Error;
use std::fmt::Write;

use dea_engine::store::StoreError;
use serde::Serialize;

/// What kind of failure a command hit, so the UI can react to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IpcErrorKind {
    /// The request broke a rule; the message says which.
    Invalid,
    /// A file could not be read or written.
    Io,
    /// Something inside Dea failed, such as a background task.
    Internal,
}

#[derive(Debug, Serialize)]
pub struct IpcError {
    pub kind: IpcErrorKind,
    /// The error and each underlying cause, as shown to the person.
    pub message: String,
}

impl IpcError {
    fn new(kind: IpcErrorKind, error: &dyn Error) -> Self {
        let mut message = error.to_string();
        let mut cause = error.source();
        while let Some(c) = cause {
            // Writing to a String cannot fail.
            let _ = write!(message, ": {c}");
            cause = c.source();
        }
        Self { kind, message }
    }
}

impl From<StoreError> for IpcError {
    fn from(e: StoreError) -> Self {
        let kind = match e {
            StoreError::Invalid(_) => IpcErrorKind::Invalid,
            StoreError::Io { .. } => IpcErrorKind::Io,
        };
        Self::new(kind, &e)
    }
}

impl From<tauri::Error> for IpcError {
    fn from(e: tauri::Error) -> Self {
        Self::new(IpcErrorKind::Internal, &e)
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    #[test]
    fn store_errors_keep_their_kind_and_their_cause() {
        let io_error = StoreError::Io {
            path: PathBuf::from("/data/settings.json"),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
        };
        let e = IpcError::from(io_error);
        assert_eq!(e.kind, IpcErrorKind::Io);
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "kind": "io", "message": "cannot write /data/settings.json: access denied" })
        );

        let e = IpcError::from(StoreError::Invalid("Font size must be 11 to 20".into()));
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "kind": "invalid", "message": "Font size must be 11 to 20" })
        );
    }
}
