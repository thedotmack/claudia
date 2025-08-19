use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Instant,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, RwLock},
};
use uuid::Uuid;

/// Status of a Claude session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    /// Session is starting up
    Starting,
    /// Session is running and processing
    Running,
    /// Session completed successfully
    Completed,
    /// Session failed with an error
    Failed,
    /// Session was cancelled by user
    Cancelled,
    /// Session was terminated (killed)
    Terminated,
}

/// Information about a running Claude session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Unique session ID
    pub id: String,
    /// Project path where Claude is running
    pub project_path: String,
    /// Model being used
    pub model: String,
    /// Initial prompt
    pub prompt: String,
    /// Current status
    pub status: SessionStatus,
    /// Process ID (if available)
    pub pid: Option<u32>,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session completed (if applicable)
    pub completed_at: Option<DateTime<Utc>>,
    /// Exit code (if completed)
    pub exit_code: Option<i32>,
    /// Claude session ID (from Claude's init message)
    pub claude_session_id: Option<String>,
    /// Accumulated output (for debugging/logging)
    pub output_preview: String,
}

/// Statistics about the process manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStats {
    /// Number of currently active sessions
    pub active_sessions: usize,
    /// Total number of sessions created
    pub total_sessions: usize,
    /// Number of completed sessions
    pub completed_sessions: usize,
    /// Number of failed sessions
    pub failed_sessions: usize,
    /// Server uptime in seconds
    pub uptime_seconds: u64,
}

/// Internal session data with process handle
struct SessionData {
    info: SessionInfo,
    child: Option<Child>,
    output_buffer: Arc<Mutex<Vec<String>>>,
}

/// Process manager for Claude sessions
pub struct ProcessManager {
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<SessionData>>>>>,
    data_dir: PathBuf,
    start_time: Instant,
    stats: Arc<RwLock<ProcessStats>>,
}

impl ProcessManager {
    /// Create a new process manager
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        // Ensure sessions directory exists
        let sessions_dir = data_dir.join("sessions");
        tokio::fs::create_dir_all(&sessions_dir).await?;

        Ok(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            data_dir,
            start_time: Instant::now(),
            stats: Arc::new(RwLock::new(ProcessStats {
                active_sessions: 0,
                total_sessions: 0,
                completed_sessions: 0,
                failed_sessions: 0,
                uptime_seconds: 0,
            })),
        })
    }

    /// Start a new Claude session
    pub async fn start_session(
        &self,
        claude_path: &str,
        project_path: String,
        prompt: String,
        model: String,
        args: Vec<String>,
    ) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();

        info!(
            "Starting new Claude session: {} in project: {}",
            session_id, project_path
        );

        // Create session info
        let session_info = SessionInfo {
            id: session_id.clone(),
            project_path: project_path.clone(),
            model: model.clone(),
            prompt: prompt.clone(),
            status: SessionStatus::Starting,
            pid: None,
            started_at: Utc::now(),
            completed_at: None,
            exit_code: None,
            claude_session_id: None,
            output_preview: String::new(),
        };

        // Build command
        let mut cmd = Command::new(claude_path);
        cmd.args(args)
            .current_dir(&project_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Add environment variables
        self.setup_command_environment(&mut cmd);

        // Spawn the process
        let child = cmd.spawn().map_err(|e| {
            error!("Failed to spawn Claude process: {}", e);
            anyhow::anyhow!("Failed to spawn Claude: {}", e)
        })?;

        let pid = child.id();
        info!("Spawned Claude process with PID: {:?}", pid);

        // Update session info with PID
        let mut updated_info = session_info;
        updated_info.pid = pid;
        updated_info.status = SessionStatus::Running;

        // Create session data
        let output_buffer = Arc::new(Mutex::new(Vec::new()));
        let session_data = SessionData {
            info: updated_info.clone(),
            child: Some(child),
            output_buffer: output_buffer.clone(),
        };

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), Arc::new(Mutex::new(session_data)));
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.active_sessions += 1;
            stats.total_sessions += 1;
        }

        // Start output monitoring in background
        self.monitor_session_output(session_id.clone(), output_buffer)
            .await;

        Ok(session_id)
    }

    /// Get information about a session
    pub async fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.read().await;
        if let Some(session_data) = sessions.get(session_id) {
            let data = session_data.lock().await;
            Some(data.info.clone())
        } else {
            None
        }
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut result = Vec::new();

        for session_data in sessions.values() {
            let data = session_data.lock().await;
            result.push(data.info.clone());
        }

        // Sort by start time (newest first)
        result.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        result
    }

    /// List only active sessions
    pub async fn list_active_sessions(&self) -> Vec<SessionInfo> {
        self.list_sessions()
            .await
            .into_iter()
            .filter(|session| {
                matches!(
                    session.status,
                    SessionStatus::Starting | SessionStatus::Running
                )
            })
            .collect()
    }

    /// Cancel a session
    pub async fn cancel_session(&self, session_id: &str) -> Result<bool> {
        let sessions = self.sessions.read().await;
        if let Some(session_data) = sessions.get(session_id) {
            let mut data = session_data.lock().await;
            
            if let Some(mut child) = data.child.take() {
                info!("Cancelling session: {}", session_id);
                
                // Update status
                data.info.status = SessionStatus::Cancelled;
                data.info.completed_at = Some(Utc::now());
                
                // Kill the process
                match child.kill().await {
                    Ok(_) => {
                        info!("Successfully killed Claude process for session: {}", session_id);
                        self.update_stats_on_completion(SessionStatus::Cancelled).await;
                        Ok(true)
                    }
                    Err(e) => {
                        error!("Failed to kill Claude process: {}", e);
                        Err(anyhow::anyhow!("Failed to kill process: {}", e))
                    }
                }
            } else {
                warn!("Session {} has no running process to cancel", session_id);
                Ok(false)
            }
        } else {
            warn!("Session not found: {}", session_id);
            Ok(false)
        }
    }

    /// Get live output from a session
    pub async fn get_session_output(&self, session_id: &str) -> Option<Vec<String>> {
        let sessions = self.sessions.read().await;
        if let Some(session_data) = sessions.get(session_id) {
            let data = session_data.lock().await;
            let output = data.output_buffer.lock().await;
            Some(output.clone())
        } else {
            None
        }
    }

    /// Get recent output from a session (last N lines)
    pub async fn get_recent_output(&self, session_id: &str, lines: usize) -> Option<Vec<String>> {
        if let Some(output) = self.get_session_output(session_id).await {
            let start = if output.len() > lines {
                output.len() - lines
            } else {
                0
            };
            Some(output[start..].to_vec())
        } else {
            None
        }
    }

    /// Get process statistics
    pub async fn get_stats(&self) -> ProcessStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime_seconds = self.start_time.elapsed().as_secs();
        stats
    }

    /// Clean up completed sessions
    pub async fn cleanup_completed_sessions(&self) -> usize {
        let mut cleaned = 0;
        let mut sessions_to_remove = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (session_id, session_data) in sessions.iter() {
                let data = session_data.lock().await;
                if matches!(
                    data.info.status,
                    SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Cancelled | SessionStatus::Terminated
                ) {
                    // Only clean up sessions that completed more than 5 minutes ago
                    if let Some(completed_at) = data.info.completed_at {
                        let age = Utc::now().signed_duration_since(completed_at);
                        if age.num_minutes() > 5 {
                            sessions_to_remove.push(session_id.clone());
                        }
                    }
                }
            }
        }

        if !sessions_to_remove.is_empty() {
            let mut sessions = self.sessions.write().await;
            for session_id in sessions_to_remove {
                sessions.remove(&session_id);
                cleaned += 1;
                debug!("Cleaned up session: {}", session_id);
            }

            // Update active session count
            let mut stats = self.stats.write().await;
            stats.active_sessions = sessions
                .values()
                .map(|data| async {
                    let data = data.lock().await;
                    matches!(
                        data.info.status,
                        SessionStatus::Starting | SessionStatus::Running
                    )
                })
                .collect::<Vec<_>>()
                .len();
        }

        if cleaned > 0 {
            info!("Cleaned up {} completed sessions", cleaned);
        }

        cleaned
    }

    /// Setup environment variables for Claude command
    fn setup_command_environment(&self, cmd: &mut Command) {
        // Inherit essential environment variables
        for (key, value) in std::env::vars() {
            if key == "PATH"
                || key == "HOME"
                || key == "USER"
                || key == "SHELL"
                || key == "LANG"
                || key.starts_with("LC_")
                || key == "NODE_PATH"
                || key == "NVM_DIR"
                || key == "NVM_BIN"
                || key == "HOMEBREW_PREFIX"
                || key == "HOMEBREW_CELLAR"
                || key == "HTTP_PROXY"
                || key == "HTTPS_PROXY"
                || key == "NO_PROXY"
                || key == "ALL_PROXY"
            {
                cmd.env(&key, &value);
            }
        }

        debug!("Environment variables set for Claude command");
    }

    /// Monitor session output in background
    async fn monitor_session_output(&self, session_id: String, output_buffer: Arc<Mutex<Vec<String>>>) {
        let sessions = self.sessions.clone();
        let stats = self.stats.clone();

        tokio::spawn(async move {
            // Get the child process
            let child = {
                let sessions = sessions.read().await;
                if let Some(session_data) = sessions.get(&session_id) {
                    let mut data = session_data.lock().await;
                    data.child.take()
                } else {
                    return;
                }
            };

            if let Some(mut child) = child {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                // Monitor stdout
                if let Some(stdout) = stdout {
                    let output_buffer_clone = output_buffer.clone();
                    let session_id_clone = session_id.clone();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stdout);
                        let mut lines = reader.lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            debug!("Session {} stdout: {}", session_id_clone, line);
                            
                            // Store in buffer
                            {
                                let mut buffer = output_buffer_clone.lock().await;
                                buffer.push(format!("[STDOUT] {}", line));
                                
                                // Keep only last 1000 lines to prevent memory issues
                                if buffer.len() > 1000 {
                                    let drain_count = buffer.len() - 1000;
                                    buffer.drain(0..drain_count);
                                }
                            }
                        }
                    });
                }

                // Monitor stderr
                if let Some(stderr) = stderr {
                    let output_buffer_clone = output_buffer.clone();
                    let session_id_clone = session_id.clone();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stderr);
                        let mut lines = reader.lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            warn!("Session {} stderr: {}", session_id_clone, line);
                            
                            // Store in buffer
                            {
                                let mut buffer = output_buffer_clone.lock().await;
                                buffer.push(format!("[STDERR] {}", line));
                                
                                // Keep only last 1000 lines
                                if buffer.len() > 1000 {
                                    let drain_count = buffer.len() - 1000;
                                    buffer.drain(0..drain_count);
                                }
                            }
                        }
                    });
                }

                // Wait for process to complete
                match child.wait().await {
                    Ok(status) => {
                        let exit_code = status.code();
                        let final_status = if status.success() {
                            SessionStatus::Completed
                        } else {
                            SessionStatus::Failed
                        };

                        info!(
                            "Session {} completed with status: {:?} (exit code: {:?})",
                            session_id, final_status, exit_code
                        );

                        // Update session info
                        let sessions = sessions.read().await;
                        if let Some(session_data) = sessions.get(&session_id) {
                            let mut data = session_data.lock().await;
                            data.info.status = final_status.clone();
                            data.info.completed_at = Some(Utc::now());
                            data.info.exit_code = exit_code;
                        }

                        // Update stats
                        {
                            let mut stats = stats.write().await;
                            stats.active_sessions = stats.active_sessions.saturating_sub(1);
                            match final_status {
                                SessionStatus::Completed => stats.completed_sessions += 1,
                                SessionStatus::Failed => stats.failed_sessions += 1,
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error waiting for session {} to complete: {}", session_id, e);
                        
                        // Update session as failed
                        let sessions = sessions.read().await;
                        if let Some(session_data) = sessions.get(&session_id) {
                            let mut data = session_data.lock().await;
                            data.info.status = SessionStatus::Failed;
                            data.info.completed_at = Some(Utc::now());
                        }

                        // Update stats
                        {
                            let mut stats = stats.write().await;
                            stats.active_sessions = stats.active_sessions.saturating_sub(1);
                            stats.failed_sessions += 1;
                        }
                    }
                }
            }
        });
    }

    /// Update statistics when a session completes
    async fn update_stats_on_completion(&self, status: SessionStatus) {
        let mut stats = self.stats.write().await;
        stats.active_sessions = stats.active_sessions.saturating_sub(1);
        match status {
            SessionStatus::Completed => stats.completed_sessions += 1,
            SessionStatus::Failed => stats.failed_sessions += 1,
            SessionStatus::Cancelled => {} // Don't count as completed or failed
            _ => {}
        }
    }
}