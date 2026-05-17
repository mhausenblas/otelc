//! Optional on-disk configuration file.
//!
//! Every field is optional; values supplied here act as defaults and are
//! overridden by command-line flags.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    /// OpAMP server WebSocket listen address.
    pub listen: Option<String>,
    /// OTLP/gRPC listen address for agent own-telemetry.
    pub otlp_listen: Option<String>,
    /// Base URL of an external OpAMP server.
    pub external_url: Option<String>,
}

/// Load a [`FileConfig`] from a YAML file.
pub fn load(path: &Path) -> anyhow::Result<FileConfig> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_yaml_ng::from_str(&text)?)
}

/// Resolve a setting: CLI flag wins, then the config file, then the default.
pub fn resolve(cli: Option<String>, file: Option<String>, default: &str) -> String {
    cli.or(file).unwrap_or_else(|| default.to_string())
}
