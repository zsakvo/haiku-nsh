//! Generic plugin package/source host and the music SDK boundary.
mod api;
pub mod contract;
mod descriptor;
mod events;
mod host;
pub mod scraper;
mod sdk;
mod source;
pub use api::*;
pub use descriptor::{Descriptor, describe_package, normalize_source_url, source_id};
pub use host::{Host, Package, SourceRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use source::{Cancellation, LoginDraft, LoginResult, QrSession, Source};

pub type Result<T> = std::result::Result<T, PluginError>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginError {
    pub code: String,
    pub message: String,
    pub stack: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub details: Option<Value>,
}
impl PluginError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            stack: String::new(),
            file: None,
            line: None,
            column: None,
            details: None,
        }
    }
}
impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for PluginError {}
impl From<std::io::Error> for PluginError {
    fn from(e: std::io::Error) -> Self {
        Self::new("IO", e.to_string())
    }
}
impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("PARSE_ERROR", e.to_string())
    }
}
impl From<haiku_jsrt::JsError> for PluginError {
    fn from(e: haiku_jsrt::JsError) -> Self {
        let code = e
            .details
            .as_ref()
            .and_then(|d| d["code"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                match e.name.as_str() {
                    "AbortError" => "CANCELLED",
                    "TimeoutError" => "TIMEOUT",
                    _ => "PLUGIN_ERROR",
                }
                .into()
            });
        Self {
            code,
            message: e.message,
            stack: e.stack,
            file: e.file,
            line: e.line,
            column: e.column,
            details: e.details,
        }
    }
}

/// Retained M0 compatibility entry point.
pub async fn plugin_echo(n: f64) -> std::result::Result<f64, String> {
    if !n.is_finite() || !(n * 2.).is_finite() {
        return Err("Invalid input".into());
    }
    let vm = haiku_jsrt::Runtime::spawn().map_err(|e| e.to_string())?;
    vm.load_module("echo.js", "export const echo=n=>n*2;")
        .await
        .map_err(|e| e.to_string())?;
    vm.call("echo", &n.to_string(), std::time::Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?
        .as_f64()
        .ok_or("Invalid result".into())
}
#[cfg(test)]
mod tests;
