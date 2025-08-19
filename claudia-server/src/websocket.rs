use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::server::ServerState;

/// WebSocket message types from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "start_session")]
    StartSession {
        data: StartSessionData,
    },
    #[serde(rename = "cancel_session")]
    CancelSession {
        session_id: String,
    },
    #[serde(rename = "get_sessions")]
    GetSessions {
        active_only: Option<bool>,
    },
    #[serde(rename = "get_output")]
    GetOutput {
        session_id: String,
        lines: Option<usize>,
    },
    #[serde(rename = "ping")]
    Ping,
}

/// Data for starting a session via WebSocket
#[derive(Debug, Deserialize)]
pub struct StartSessionData {
    pub project_path: String,
    pub prompt: String,
    pub model: Option<String>,
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub continue_conversation: bool,
    pub session_id: Option<String>,
}

/// WebSocket message types to client
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "session_started")]
    SessionStarted {
        session_id: String,
        message: String,
    },
    #[serde(rename = "session_output")]
    SessionOutput {
        session_id: String,
        line: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    #[serde(rename = "session_completed")]
    SessionCompleted {
        session_id: String,
        status: String,
        exit_code: Option<i32>,
    },
    #[serde(rename = "session_cancelled")]
    SessionCancelled {
        session_id: String,
    },
    #[serde(rename = "sessions_list")]
    SessionsList {
        sessions: Vec<crate::process::SessionInfo>,
    },
    #[serde(rename = "session_output_history")]
    SessionOutputHistory {
        session_id: String,
        output: Vec<String>,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
        code: Option<String>,
    },
    #[serde(rename = "pong")]
    Pong,
}

/// WebSocket handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Handle WebSocket connection
async fn handle_websocket(mut socket: WebSocket, state: ServerState) {
    info!("New WebSocket connection established");

    // Create channels for communication
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();
    
    // Track active sessions for this client
    let mut active_sessions: HashMap<String, mpsc::UnboundedSender<()>> = HashMap::new();

    // Send welcome message
    let welcome = ServerMessage::SessionsList {
        sessions: state.process_manager.list_sessions().await,
    };
    
    if let Err(e) = send_message(&mut socket, welcome).await {
        error!("Failed to send welcome message: {}", e);
        return;
    }

    // Main message loop
    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        debug!("Received WebSocket message: {}", text);
                        
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                if let Err(e) = handle_client_message(
                                    client_msg,
                                    &state,
                                    &tx,
                                    &mut active_sessions,
                                ).await {
                                    error!("Error handling client message: {}", e);
                                    let error_msg = ServerMessage::Error {
                                        message: e.to_string(),
                                        code: Some("HANDLER_ERROR".to_string()),
                                    };
                                    let _ = send_message(&mut socket, error_msg).await;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse client message: {}", e);
                                let error_msg = ServerMessage::Error {
                                    message: format!("Invalid message format: {}", e),
                                    code: Some("PARSE_ERROR".to_string()),
                                };
                                let _ = send_message(&mut socket, error_msg).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket connection closed by client");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket connection ended");
                        break;
                    }
                    _ => {
                        // Ignore other message types (binary, ping, pong)
                    }
                }
            }
            
            // Handle outgoing messages to client
            msg = rx.recv() => {
                match msg {
                    Some(server_msg) => {
                        if let Err(e) = send_message(&mut socket, server_msg).await {
                            error!("Failed to send message to client: {}", e);
                            break;
                        }
                    }
                    None => {
                        debug!("Message channel closed");
                        break;
                    }
                }
            }
        }
    }

    // Cleanup: cancel any active sessions for this client
    for (session_id, cancel_tx) in active_sessions {
        info!("Cleaning up session {} for disconnected client", session_id);
        let _ = cancel_tx.send(());
    }

    info!("WebSocket connection handler finished");
}

/// Handle a client message
async fn handle_client_message(
    message: ClientMessage,
    state: &ServerState,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    active_sessions: &mut HashMap<String, mpsc::UnboundedSender<()>>,
) -> anyhow::Result<()> {
    match message {
        ClientMessage::StartSession { data } => {
            handle_start_session(data, state, tx, active_sessions).await?;
        }
        ClientMessage::CancelSession { session_id } => {
            handle_cancel_session(session_id, state, tx, active_sessions).await?;
        }
        ClientMessage::GetSessions { active_only } => {
            handle_get_sessions(active_only.unwrap_or(false), state, tx).await?;
        }
        ClientMessage::GetOutput { session_id, lines } => {
            handle_get_output(session_id, lines, state, tx).await?;
        }
        ClientMessage::Ping => {
            tx.send(ServerMessage::Pong)?;
        }
    }
    Ok(())
}

/// Handle start session request
async fn handle_start_session(
    data: StartSessionData,
    state: &ServerState,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    active_sessions: &mut HashMap<String, mpsc::UnboundedSender<()>>,
) -> anyhow::Result<()> {
    // Validate project path
    if !std::path::Path::new(&data.project_path).exists() {
        return Err(anyhow::anyhow!("Project path does not exist: {}", data.project_path));
    }

    let model = data.model.unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
    
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
    args.extend(vec!["-p".to_string(), data.prompt.clone()]);

    // Add continue flag if specified
    if data.continue_conversation {
        args.insert(0, "-c".to_string());
    }

    // Add resume flag if session ID provided
    if let Some(session_id) = data.session_id {
        args.extend(vec!["--resume".to_string(), session_id]);
    }

    // Add any additional args
    if let Some(additional_args) = data.args {
        args.extend(additional_args);
    }

    // Start the session
    let session_id = state
        .process_manager
        .start_session(
            state.claude_binary.path(),
            data.project_path,
            data.prompt,
            model,
            args,
        )
        .await?;

    // Send session started message
    tx.send(ServerMessage::SessionStarted {
        session_id: session_id.clone(),
        message: "Session started successfully".to_string(),
    })?;

    // Start monitoring session output
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
    active_sessions.insert(session_id.clone(), cancel_tx);
    
    monitor_session_output(session_id.clone(), state.clone(), tx.clone(), cancel_rx).await;

    Ok(())
}

/// Handle cancel session request
async fn handle_cancel_session(
    session_id: String,
    state: &ServerState,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    active_sessions: &mut HashMap<String, mpsc::UnboundedSender<()>>,
) -> anyhow::Result<()> {
    // Cancel in process manager
    match state.process_manager.cancel_session(&session_id).await {
        Ok(true) => {
            tx.send(ServerMessage::SessionCancelled { session_id: session_id.clone() })?;
        }
        Ok(false) => {
            tx.send(ServerMessage::Error {
                message: "Session was not running or already completed".to_string(),
                code: Some("SESSION_NOT_ACTIVE".to_string()),
            })?;
        }
        Err(e) => {
            tx.send(ServerMessage::Error {
                message: format!("Failed to cancel session: {}", e),
                code: Some("CANCEL_FAILED".to_string()),
            })?;
        }
    }

    // Remove from active sessions
    if let Some(cancel_tx) = active_sessions.remove(&session_id) {
        let _ = cancel_tx.send(());
    }

    Ok(())
}

/// Handle get sessions request
async fn handle_get_sessions(
    active_only: bool,
    state: &ServerState,
    tx: &mpsc::UnboundedSender<ServerMessage>,
) -> anyhow::Result<()> {
    let sessions = if active_only {
        state.process_manager.list_active_sessions().await
    } else {
        state.process_manager.list_sessions().await
    };

    tx.send(ServerMessage::SessionsList { sessions })?;
    Ok(())
}

/// Handle get output request
async fn handle_get_output(
    session_id: String,
    lines: Option<usize>,
    state: &ServerState,
    tx: &mpsc::UnboundedSender<ServerMessage>,
) -> anyhow::Result<()> {
    let output = if let Some(lines) = lines {
        state.process_manager.get_recent_output(&session_id, lines).await
    } else {
        state.process_manager.get_session_output(&session_id).await
    };

    match output {
        Some(output_lines) => {
            tx.send(ServerMessage::SessionOutputHistory {
                session_id,
                output: output_lines,
            })?;
        }
        None => {
            tx.send(ServerMessage::Error {
                message: "Session not found".to_string(),
                code: Some("SESSION_NOT_FOUND".to_string()),
            })?;
        }
    }

    Ok(())
}

/// Monitor session output and send updates via WebSocket
async fn monitor_session_output(
    session_id: String,
    state: ServerState,
    tx: mpsc::UnboundedSender<ServerMessage>,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
) {
    let session_id_clone = session_id.clone();
    
    tokio::spawn(async move {
        let mut last_line_count = 0;
        let mut monitoring = true;

        while monitoring {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    debug!("Stopping output monitoring for session: {}", session_id);
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                    // Check for new output
                    if let Some(output) = state.process_manager.get_session_output(&session_id).await {
                        if output.len() > last_line_count {
                            // Send new lines
                            for line in &output[last_line_count..] {
                                let msg = ServerMessage::SessionOutput {
                                    session_id: session_id.clone(),
                                    line: line.clone(),
                                    timestamp: chrono::Utc::now(),
                                };
                                
                                if tx.send(msg).is_err() {
                                    debug!("Client disconnected, stopping output monitoring");
                                    monitoring = false;
                                    break;
                                }
                            }
                            last_line_count = output.len();
                        }
                    }

                    // Check if session completed
                    if let Some(session_info) = state.process_manager.get_session(&session_id).await {
                        if !matches!(session_info.status, crate::process::SessionStatus::Starting | crate::process::SessionStatus::Running) {
                            let msg = ServerMessage::SessionCompleted {
                                session_id: session_id.clone(),
                                status: format!("{:?}", session_info.status),
                                exit_code: session_info.exit_code,
                            };
                            
                            let _ = tx.send(msg);
                            monitoring = false;
                        }
                    } else {
                        // Session no longer exists
                        monitoring = false;
                    }
                }
            }
        }

        debug!("Output monitoring finished for session: {}", session_id_clone);
    });
}

/// Send a message via WebSocket
async fn send_message(socket: &mut WebSocket, message: ServerMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(&message).map_err(|e| {
        axum::Error::new(format!("Failed to serialize message: {}", e))
    })?;
    
    socket.send(Message::Text(json)).await
}