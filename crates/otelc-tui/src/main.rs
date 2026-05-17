//! `otelc` — a Norton Commander-style TUI for managing OpenTelemetry
//! Collectors over the Open Agent Management Protocol (OpAMP).

mod app;
mod cli;
mod config;
mod control;
mod pipeline;
mod theme;
mod ui;
mod views;

use clap::Parser;
use cli::{Cli, Mode};
use control::embedded::EmbeddedControlPlane;
use control::external::ExternalControlPlane;
use tracing_appender::non_blocking::WorkerGuard;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _guard = init_logging(&cli.log_file)?;

    let file_cfg = match &cli.config {
        Some(path) => config::load(path)?,
        None => config::FileConfig::default(),
    };

    let result = match cli.mode {
        Mode::Embedded => {
            let listen = config::resolve(cli.listen, file_cfg.listen, "127.0.0.1:4320");
            let otlp = config::resolve(cli.otlp_listen, file_cfg.otlp_listen, "127.0.0.1:4317");
            let (control, rx) = EmbeddedControlPlane::start(listen.parse()?, otlp.parse()?).await?;
            app::run(control, rx).await
        }
        Mode::External => {
            let url = config::resolve(
                cli.external_url,
                file_cfg.external_url,
                "http://127.0.0.1:8080",
            );
            let (control, rx) = ExternalControlPlane::start(url);
            app::run(control, rx).await
        }
    };

    ratatui::restore();
    result
}

fn init_logging(path: &std::path::Path) -> anyhow::Result<WorkerGuard> {
    let file = std::fs::File::create(path)?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .init();
    Ok(guard)
}
