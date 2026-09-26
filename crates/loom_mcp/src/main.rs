//! `loom-mcp`: an MCP server (stdio transport) that drives a headless
//! `loom serve` child process, one tool per serve method
//! (`docs/architecture.md` section 13).
//!
//! stdout belongs to the MCP transport: nothing else may write to it. Logs
//! go to stderr, and so does the child's stderr.

mod bridge;
mod reply;
mod server;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use rmcp::ServiceExt;

use crate::bridge::Bridge;
use crate::server::LoomServer;

#[derive(Debug, Parser)]
#[command(
    name = "loom-mcp",
    version,
    about = "MCP server for a headless loom session"
)]
struct Args {
    /// The `loom` executable. Default: `loom` next to this executable.
    #[arg(long, value_name = "PATH")]
    loom: Option<PathBuf>,
    /// Default screenshot size, passed to `loom serve --size`.
    #[arg(long, value_name = "WxH")]
    size: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    let args = Args::parse();
    let loom = match args.loom {
        Some(loom) => loom,
        None => default_loom()?,
    };
    log::info!("loom-mcp ready; loom serve is {}", loom.display());

    let server = LoomServer::new(Bridge::new(loom, args.size));
    let running = server
        .clone()
        .serve(rmcp::transport::stdio())
        .await
        .context("starting the MCP session")?;
    let reason = running.waiting().await;
    log::info!("MCP session ended: {reason:?}");
    server.shutdown().await;
    Ok(())
}

/// `loom` (`loom.exe` on Windows) in the directory of this executable.
fn default_loom() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("locating loom-mcp itself")?;
    let dir = exe
        .parent()
        .context("loom-mcp's own path has no parent directory")?;
    Ok(dir.join(format!("loom{}", std::env::consts::EXE_SUFFIX)))
}
