use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::server::ServerState;

/// Request to start a new Claude session
#[derive(Debug, Deserialize)]
pub struct StartSessionRequest {
    pub project_path: String,
    pub prompt: String,
    pub model: Option<String>,
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub continue_conversation: bool,
    pub session_id: Option<String>, // For resuming
}

/// Response when starting a session
#[derive(Debug, Serialize)]
pub struct StartSessionResponse {
    pub session_id: String,
    pub message: String,
}

/// Query parameters for listing sessions
#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub active_only: Option<bool>,
    pub limit: Option<usize>,
}

/// Query parameters for getting session output
#[derive(Debug, Deserialize)]
pub struct OutputQuery {
    pub lines: Option<usize>,
    pub format: Option<String>, // "json" or "text"
}

/// API routes handler
pub struct ApiRoutes;

impl ApiRoutes {
    /// Create the API router
    pub fn create_router() -> Router<ServerState> {
        Router::new()
            // Session management
            .route("/sessions", post(start_session))
            .route("/sessions", get(list_sessions))
            .route("/sessions/:id", get(get_session))
            .route("/sessions/:id", delete(cancel_session))
            .route("/sessions/:id/output", get(get_session_output))
            
            // Claude binary information
            .route("/claude/info", get(get_claude_info))
            .route("/claude/installations", get(list_claude_installations))
            .route("/claude/version", get(get_claude_version))
            
            // Process management
            .route("/processes/stats", get(get_process_stats))
            .route("/processes/cleanup", post(cleanup_processes))
            
            // Examples and templates
            .route("/examples", get(get_examples))
            .route("/examples/:name", get(get_example))
    }
}

/// Start a new Claude session
async fn start_session(
    State(state): State<ServerState>,
    Json(req): Json<StartSessionRequest>,
) -> Result<Json<StartSessionResponse>, StatusCode> {
    info!(
        "Starting new session for project: {}, model: {:?}",
        req.project_path,
        req.model
    );

    // Validate project path
    if !std::path::Path::new(&req.project_path).exists() {
        warn!("Project path does not exist: {}", req.project_path);
        return Err(StatusCode::BAD_REQUEST);
    }

    let model = req.model.unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
    
    // Build Claude command arguments
    let mut args = vec![
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--model".to_string(),
        model.clone(),
    ];

    // Add prompt
    args.extend(vec!["-p".to_string(), req.prompt.clone()]);

    // Add continue flag if specified
    if req.continue_conversation {
        args.insert(0, "-c".to_string());
    }

    // Add resume flag if session ID provided
    if let Some(session_id) = req.session_id {
        args.extend(vec!["--resume".to_string(), session_id]);
    }

    // Add any additional args
    if let Some(additional_args) = req.args {
        args.extend(additional_args);
    }

    match state
        .process_manager
        .start_session(
            state.claude_binary.path(),
            req.project_path,
            req.prompt,
            model,
            args,
        )
        .await
    {
        Ok(session_id) => {
            info!("Started session: {}", session_id);
            Ok(Json(StartSessionResponse {
                session_id,
                message: "Session started successfully".to_string(),
            }))
        }
        Err(e) => {
            error!("Failed to start session: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List sessions
async fn list_sessions(
    State(state): State<ServerState>,
    Query(query): Query<ListSessionsQuery>,
) -> Json<Value> {
    let sessions = if query.active_only.unwrap_or(false) {
        state.process_manager.list_active_sessions().await
    } else {
        state.process_manager.list_sessions().await
    };

    let mut result = sessions;
    
    // Apply limit if specified
    if let Some(limit) = query.limit {
        result.truncate(limit);
    }

    Json(json!({
        "sessions": result,
        "count": result.len()
    }))
}

/// Get specific session
async fn get_session(
    State(state): State<ServerState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match state.process_manager.get_session(&session_id).await {
        Some(session) => Ok(Json(json!(session))),
        None => {
            warn!("Session not found: {}", session_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Cancel a session
async fn cancel_session(
    State(state): State<ServerState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Cancelling session: {}", session_id);

    match state.process_manager.cancel_session(&session_id).await {
        Ok(true) => Ok(Json(json!({
            "success": true,
            "message": "Session cancelled successfully"
        }))),
        Ok(false) => Ok(Json(json!({
            "success": false,
            "message": "Session was not running or already completed"
        }))),
        Err(e) => {
            error!("Failed to cancel session {}: {}", session_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get session output
async fn get_session_output(
    State(state): State<ServerState>,
    Path(session_id): Path<String>,
    Query(query): Query<OutputQuery>,
) -> Result<Json<Value>, StatusCode> {
    let output = if let Some(lines) = query.lines {
        state.process_manager.get_recent_output(&session_id, lines).await
    } else {
        state.process_manager.get_session_output(&session_id).await
    };

    match output {
        Some(lines) => {
            let format = query.format.as_deref().unwrap_or("json");
            match format {
                "text" => Ok(Json(json!({
                    "output": lines.join("\n"),
                    "format": "text",
                    "line_count": lines.len()
                }))),
                _ => Ok(Json(json!({
                    "output": lines,
                    "format": "json",
                    "line_count": lines.len()
                }))),
            }
        }
        None => {
            warn!("Session output not found: {}", session_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Get Claude binary information
async fn get_claude_info(
    State(state): State<ServerState>,
) -> Json<Value> {
    let installation = state.claude_binary.installation();
    
    Json(json!({
        "path": installation.path,
        "version": installation.version,
        "source": installation.source,
        "installation_type": installation.installation_type,
        "available": state.claude_binary.is_available().await
    }))
}

/// List all detected Claude installations
async fn list_claude_installations() -> Json<Value> {
    let installations = crate::claude::ClaudeBinary::discover_all_installations().await;
    
    Json(json!({
        "installations": installations,
        "count": installations.len()
    }))
}

/// Get Claude version
async fn get_claude_version(
    State(state): State<ServerState>,
) -> Result<Json<Value>, StatusCode> {
    match state.claude_binary.get_version().await {
        Ok(version) => Ok(Json(json!({
            "version": version,
            "path": state.claude_binary.path()
        }))),
        Err(e) => {
            error!("Failed to get Claude version: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get process statistics
async fn get_process_stats(
    State(state): State<ServerState>,
) -> Json<Value> {
    let stats = state.process_manager.get_stats().await;
    Json(json!(stats))
}

/// Cleanup completed processes
async fn cleanup_processes(
    State(state): State<ServerState>,
) -> Json<Value> {
    let cleaned = state.process_manager.cleanup_completed_sessions().await;
    
    Json(json!({
        "cleaned": cleaned,
        "message": format!("Cleaned up {} completed sessions", cleaned)
    }))
}

/// Get examples of how to use the API
async fn get_examples() -> Json<Value> {
    Json(json!({
        "examples": [
            {
                "name": "basic",
                "description": "Basic session with a simple prompt",
                "url": "/api/examples/basic"
            },
            {
                "name": "continue",
                "description": "Continue an existing conversation",
                "url": "/api/examples/continue"
            },
            {
                "name": "resume",
                "description": "Resume a specific session",
                "url": "/api/examples/resume"
            },
            {
                "name": "websocket",
                "description": "WebSocket streaming example",
                "url": "/api/examples/websocket"
            }
        ]
    }))
}

/// Get specific example
async fn get_example(
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let example = match name.as_str() {
        "basic" => json!({
            "name": "basic",
            "description": "Start a basic Claude session",
            "method": "POST",
            "endpoint": "/api/sessions",
            "headers": {
                "Content-Type": "application/json"
            },
            "body": {
                "project_path": "/path/to/your/project",
                "prompt": "Help me write a Python script to process CSV files",
                "model": "claude-3-5-sonnet-20241022"
            },
            "curl_example": "curl -X POST http://localhost:3030/api/sessions \\\n  -H 'Content-Type: application/json' \\\n  -d '{\n    \"project_path\": \"/path/to/your/project\",\n    \"prompt\": \"Help me write a Python script\",\n    \"model\": \"claude-3-5-sonnet-20241022\"\n  }'"
        }),
        "continue" => json!({
            "name": "continue",
            "description": "Continue an existing conversation",
            "method": "POST",
            "endpoint": "/api/sessions",
            "headers": {
                "Content-Type": "application/json"
            },
            "body": {
                "project_path": "/path/to/your/project",
                "prompt": "Now add error handling to the script",
                "model": "claude-3-5-sonnet-20241022",
                "continue_conversation": true
            },
            "curl_example": "curl -X POST http://localhost:3030/api/sessions \\\n  -H 'Content-Type: application/json' \\\n  -d '{\n    \"project_path\": \"/path/to/your/project\",\n    \"prompt\": \"Add error handling\",\n    \"continue_conversation\": true\n  }'"
        }),
        "resume" => json!({
            "name": "resume",
            "description": "Resume a specific session by ID",
            "method": "POST",
            "endpoint": "/api/sessions",
            "headers": {
                "Content-Type": "application/json"
            },
            "body": {
                "project_path": "/path/to/your/project",
                "prompt": "Continue working on this",
                "model": "claude-3-5-sonnet-20241022",
                "session_id": "uuid-of-existing-session"
            },
            "curl_example": "curl -X POST http://localhost:3030/api/sessions \\\n  -H 'Content-Type: application/json' \\\n  -d '{\n    \"project_path\": \"/path/to/your/project\",\n    \"prompt\": \"Continue working\",\n    \"session_id\": \"your-session-id\"\n  }'"
        }),
        "websocket" => json!({
            "name": "websocket",
            "description": "Real-time streaming via WebSocket",
            "endpoint": "ws://localhost:3030/ws",
            "protocol": "WebSocket",
            "message_format": {
                "type": "start_session",
                "data": {
                    "project_path": "/path/to/your/project",
                    "prompt": "Help me debug this code",
                    "model": "claude-3-5-sonnet-20241022"
                }
            },
            "javascript_example": "const ws = new WebSocket('ws://localhost:3030/ws');\nws.onopen = () => {\n  ws.send(JSON.stringify({\n    type: 'start_session',\n    data: {\n      project_path: '/path/to/project',\n      prompt: 'Help me with this code',\n      model: 'claude-3-5-sonnet-20241022'\n    }\n  }));\n};\nws.onmessage = (event) => {\n  const message = JSON.parse(event.data);\n  console.log('Received:', message);\n};"
        }),
        _ => return Err(StatusCode::NOT_FOUND),
    };

    Ok(Json(example))
}