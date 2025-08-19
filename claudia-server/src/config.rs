use anyhow::Result;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server settings
    pub server: ServerSettings,
    /// Claude settings
    pub claude: ClaudeSettings,
    /// Process management settings
    pub process: ProcessSettings,
    /// Logging settings
    pub logging: LoggingSettings,
}

/// Server-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Maximum number of concurrent sessions
    pub max_concurrent_sessions: usize,
    /// Session timeout in seconds
    pub session_timeout_seconds: u64,
    /// Whether to clean up completed sessions automatically
    pub auto_cleanup: bool,
    /// Cleanup interval in seconds
    pub cleanup_interval_seconds: u64,
}

/// Claude-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSettings {
    /// Default model to use
    pub default_model: String,
    /// Default arguments to pass to Claude
    pub default_args: Vec<String>,
    /// Whether to enable verbose output by default
    pub verbose: bool,
    /// Whether to skip permissions by default (dangerous)
    pub skip_permissions: bool,
}

/// Process management settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSettings {
    /// Maximum output lines to keep in memory per session
    pub max_output_lines: usize,
    /// Whether to save session output to disk
    pub save_output_to_disk: bool,
    /// Directory to save session output (relative to data dir)
    pub output_directory: String,
}

/// Logging settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSettings {
    /// Log level (error, warn, info, debug, trace)
    pub level: String,
    /// Whether to log to file
    pub log_to_file: bool,
    /// Log file path (relative to data dir)
    pub log_file: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server: ServerSettings {
                max_concurrent_sessions: 10,
                session_timeout_seconds: 3600, // 1 hour
                auto_cleanup: true,
                cleanup_interval_seconds: 300, // 5 minutes
            },
            claude: ClaudeSettings {
                default_model: "claude-3-5-sonnet-20241022".to_string(),
                default_args: vec![
                    "--output-format".to_string(),
                    "stream-json".to_string(),
                    "--verbose".to_string(),
                    "--dangerously-skip-permissions".to_string(),
                ],
                verbose: true,
                skip_permissions: true,
            },
            process: ProcessSettings {
                max_output_lines: 1000,
                save_output_to_disk: true,
                output_directory: "sessions".to_string(),
            },
            logging: LoggingSettings {
                level: "info".to_string(),
                log_to_file: true,
                log_file: "claudia-server.log".to_string(),
            },
        }
    }
}

impl ServerConfig {
    /// Load configuration from file or create default
    pub async fn load(config_file: Option<String>, data_dir: &PathBuf) -> Result<Self> {
        if let Some(config_path) = config_file {
            info!("Loading configuration from: {}", config_path);
            Self::load_from_file(config_path).await
        } else {
            let default_config_path = data_dir.join("config.toml");
            if default_config_path.exists() {
                info!("Loading configuration from: {}", default_config_path.display());
                Self::load_from_file(default_config_path.to_string_lossy().to_string()).await
            } else {
                info!("No configuration file found, using defaults");
                let config = Self::default();
                // Save default config for future reference
                if let Err(e) = config.save_to_file(&default_config_path).await {
                    warn!("Failed to save default configuration: {}", e);
                }
                Ok(config)
            }
        }
    }

    /// Load configuration from a specific file
    async fn load_from_file(path: String) -> Result<Self> {
        let content = fs::read_to_string(&path).await?;
        let config: Self = toml::from_str(&content)?;
        info!("Configuration loaded successfully from: {}", path);
        Ok(config)
    }

    /// Save configuration to file
    pub async fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).await?;
        info!("Configuration saved to: {}", path.display());
        Ok(())
    }

    /// Get the full output directory path
    pub fn output_dir(&self, data_dir: &PathBuf) -> PathBuf {
        data_dir.join(&self.process.output_directory)
    }

    /// Get the full log file path
    pub fn log_file_path(&self, data_dir: &PathBuf) -> PathBuf {
        data_dir.join(&self.logging.log_file)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.server.max_concurrent_sessions == 0 {
            return Err(anyhow::anyhow!("max_concurrent_sessions must be greater than 0"));
        }

        if self.server.session_timeout_seconds == 0 {
            return Err(anyhow::anyhow!("session_timeout_seconds must be greater than 0"));
        }

        if self.process.max_output_lines == 0 {
            return Err(anyhow::anyhow!("max_output_lines must be greater than 0"));
        }

        // Validate log level
        match self.logging.level.to_lowercase().as_str() {
            "error" | "warn" | "info" | "debug" | "trace" => {}
            _ => return Err(anyhow::anyhow!("Invalid log level: {}", self.logging.level)),
        }

        Ok(())
    }

    /// Get Claude command arguments with defaults
    pub fn get_claude_args(&self, custom_args: Option<Vec<String>>) -> Vec<String> {
        let mut args = self.claude.default_args.clone();
        
        if let Some(custom) = custom_args {
            // Merge custom args, avoiding duplicates
            for arg in custom {
                if !args.contains(&arg) {
                    args.push(arg);
                }
            }
        }
        
        args
    }
}