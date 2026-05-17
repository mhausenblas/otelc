//! Command-line interface.

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Norton Commander-style TUI for managing OpenTelemetry Collectors via OpAMP.
#[derive(Parser, Debug)]
#[command(name = "otelc", version, about)]
pub struct Cli {
    /// Operating mode.
    #[arg(long, value_enum, default_value_t = Mode::Embedded)]
    pub mode: Mode,

    /// OpAMP server WebSocket listen address (embedded mode).
    #[arg(long)]
    pub listen: Option<String>,

    /// OTLP/gRPC listen address for agent own-telemetry (embedded mode).
    #[arg(long)]
    pub otlp_listen: Option<String>,

    /// Base URL of an external OpAMP server (external mode).
    #[arg(long)]
    pub external_url: Option<String>,

    /// Optional YAML config file supplying defaults.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Log file path (the TUI owns stdout, so logs go to a file).
    #[arg(long, default_value = "otelc.log")]
    pub log_file: PathBuf,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Run an embedded OpAMP server that collectors connect to directly.
    Embedded,
    /// Connect to an external OpAMP server via an adapter.
    External,
}
