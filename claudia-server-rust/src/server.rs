use anyhow::Result;
use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use log::{error, info};
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::cors::CorsLayer;

use crate::{
    api::ApiRoutes, claude::ClaudeBinary, config::ServerConfig, process::ProcessManager,
    websocket::ws_handler,
};

/// Main server state shared across handlers
#[derive(Clone)]
pub struct ServerState {
    pub claude_binary: Arc<ClaudeBinary>,
    pub process_manager: Arc<ProcessManager>,
    pub config: Arc<ServerConfig>,
}

/// The main Claudia server
pub struct ClaudiaServer {
    state: ServerState,
    data_dir: PathBuf,
}

impl ClaudiaServer {
    /// Create a new server instance
    pub async fn new(
        claude_path: Option<String>,
        data_dir: Option<String>,
        config_file: Option<String>,
    ) -> Result<Self> {
        // Determine data directory
        let data_dir = if let Some(dir) = data_dir {
            PathBuf::from(dir)
        } else {
            dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                .join(".claudia-server")
        };

        // Ensure data directory exists
        tokio::fs::create_dir_all(&data_dir).await?;
        info!("Using data directory: {}", data_dir.display());

        // Load configuration
        let config = ServerConfig::load(config_file, &data_dir).await?;
        config.validate()?;

        // Initialize Claude binary detector
        let claude_binary = ClaudeBinary::new(claude_path).await?;
        info!("Using Claude binary: {}", claude_binary.path());

        // Initialize process manager
        let process_manager = ProcessManager::new(data_dir.clone()).await?;

        // Ensure output directory exists
        let output_dir = config.output_dir(&data_dir);
        tokio::fs::create_dir_all(&output_dir).await?;
        info!("Using output directory: {}", output_dir.display());

        let state = ServerState {
            claude_binary: Arc::new(claude_binary),
            process_manager: Arc::new(process_manager),
            config: Arc::new(config),
        };

        Ok(Self { state, data_dir })
    }

    /// Get the Claude binary path
    pub fn claude_path(&self) -> &str {
        self.state.claude_binary.path()
    }

    /// Get the data directory
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// Start the server
    pub async fn start(self, addr: SocketAddr) -> Result<()> {
        let app = self.create_router();

        info!("Starting server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Create the router with all routes
    fn create_router(self) -> Router {
        Router::new()
            // Health check
            .route("/health", get(health_check))
            // API endpoints
            .nest("/api", ApiRoutes::create_router())
            // WebSocket endpoint
            .route("/ws", get(ws_handler))
            // Server info
            .route("/info", get(server_info))
            // Add CORS middleware
            .layer(CorsLayer::permissive())
            // Add shared state
            .with_state(self.state)
    }
}

/// Health check endpoint
async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": "claudia-server",
        "version": "0.1.0"
    }))
}

/// Server info endpoint
async fn server_info(State(state): State<ServerState>) -> Result<Json<Value>, StatusCode> {
    let claude_info = match state.claude_binary.get_version().await {
        Ok(version) => json!({
            "path": state.claude_binary.path(),
            "version": version,
            "available": true
        }),
        Err(e) => {
            error!("Failed to get Claude version: {}", e);
            json!({
                "path": state.claude_binary.path(),
                "version": null,
                "available": false,
                "error": e.to_string()
            })
        }
    };

    let process_stats = state.process_manager.get_stats().await;
    let data_dir = state.process_manager.data_dir();
    let log_file = state.config.log_file_path(data_dir);

    Ok(Json(json!({
        "service": "claudia-server",
        "version": "0.1.0",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "claude": claude_info,
        "data_directory": data_dir.display().to_string(),
        "log_file": log_file.display().to_string(),
        "processes": {
            "active_sessions": process_stats.active_sessions,
            "total_sessions": process_stats.total_sessions,
            "completed_sessions": process_stats.completed_sessions,
            "failed_sessions": process_stats.failed_sessions
        },
        "uptime_seconds": process_stats.uptime_seconds
    })))
}
