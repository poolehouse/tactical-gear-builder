//! JSON codec helpers for the service boundary.
//!
//! `core-engine` performs no network I/O itself; these helpers give the host
//! service a single serialization path for specs and results.

use serde::{de::DeserializeOwned, Serialize};

/// Serialize an engine value to a compact JSON string.
pub fn to_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

/// Serialize an engine value to pretty JSON (exports, debugging).
pub fn to_json_pretty<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// Parse a JSON string back into an engine value.
pub fn from_json<T: DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(json)
}
