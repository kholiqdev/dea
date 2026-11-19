//! Helpers for Dea tests. Use only under `[dev-dependencies]`.

mod canary;
#[cfg(feature = "http")]
pub mod http;

pub use canary::{canary, find_canary};
