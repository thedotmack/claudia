use anyhow::Result;
use clap::{Arg, Command};
use log::{error, info};
use std::net::SocketAddr;

mod api;
mod claude;
mod config;
mod process;
mod server;
mod websocket;

use server::ClaudiaServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    let matches = Command::new("claudia-server")
        .version("0.1.0")
        .about("Standalone server wrapper for Claude Code CLI")
        .arg(
            Arg::new("host")
                .long("host")
                .value_name("HOST")
                .help("Host to bind the server to")
                .default_value("127.0.0.1"),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .short('p')
                .value_name("PORT")
                .help("Port to bind the server to")
                .default_value("3030"),
        )
        .arg(
            Arg::new("claude-path")
                .long("claude-path")
                .value_name("PATH")
                .help("Path to Claude CLI binary (auto-detected if not provided)"),
        )
        .arg(
            Arg::new("data-dir")
                .long("data-dir")
                .value_name("DIR")
                .help("Directory to store server data (defaults to ~/.claudia-server)"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .short('c')
                .value_name("FILE")
                .help("Configuration file path"),
        )
        .get_matches();

    let host = matches.get_one::<String>("host").unwrap();
    let port = matches.get_one::<String>("port").unwrap();
    let claude_path = matches.get_one::<String>("claude-path").cloned();
    let data_dir = matches.get_one::<String>("data-dir").cloned();
    let config_file = matches.get_one::<String>("config").cloned();

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid host:port combination: {}", e))?;

    info!("Starting Claudia Server v0.1.0");
    info!("Binding to: {}", addr);

    // Create and start the server
    let server = ClaudiaServer::new(claude_path, data_dir, config_file).await?;

    info!("Server configuration:");
    info!("  - Claude binary: {}", server.claude_path());
    info!("  - Data directory: {}", server.data_dir().display());
    info!("  - API endpoints available at: http://{}/api", addr);
    info!("  - WebSocket endpoint available at: ws://{}/ws", addr);
    info!("  - Health check at: http://{}/health", addr);

    match server.start(addr).await {
        Ok(_) => {
            info!("Server started successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to start server: {}", e);
            Err(e)
        }
    }
}
