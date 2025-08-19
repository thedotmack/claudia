# Claudia Server

A standalone server wrapper for Claude Code CLI that provides HTTP and WebSocket APIs for remote interaction with Claude Code sessions.

## Features

- **Standalone Operation**: Runs independently without Tauri GUI
- **HTTP REST API**: Complete session management via REST endpoints
- **WebSocket Streaming**: Real-time output streaming via WebSocket connections
- **Multi-Session Support**: Concurrent Claude sessions with process isolation
- **Auto-Discovery**: Automatic Claude binary detection across multiple installation methods
- **Process Management**: Robust subprocess management with proper cleanup
- **Configuration**: Flexible configuration via TOML files
- **Session Persistence**: Session history and output management

## Quick Start

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/thedotmack/claudia.git
   cd claudia/claudia-server-rust
   ```

2. Build the server:
   ```bash
   cargo build --release
   ```

3. Run the server:
   ```bash
   ./target/release/claudia-server
   ```

The server will start on `http://localhost:3030` by default.

### Basic Usage

#### Start a Claude Session (REST API)

```bash
curl -X POST http://localhost:3030/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{
    "project_path": "/path/to/your/project",
    "prompt": "Help me write a Python script to process CSV files",
    "model": "claude-3-5-sonnet-20241022"
  }'
```

Response:
```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "message": "Session started successfully"
}
```

#### Get Session Output

```bash
curl http://localhost:3030/api/sessions/550e8400-e29b-41d4-a716-446655440000/output
```

#### List All Sessions

```bash
curl http://localhost:3030/api/sessions
```

#### Cancel a Session

```bash
curl -X DELETE http://localhost:3030/api/sessions/550e8400-e29b-41d4-a716-446655440000
```

## API Reference

### REST API Endpoints

#### Sessions

- **POST** `/api/sessions` - Start a new Claude session
- **GET** `/api/sessions` - List all sessions
- **GET** `/api/sessions/:id` - Get session details
- **DELETE** `/api/sessions/:id` - Cancel a session
- **GET** `/api/sessions/:id/output` - Get session output

#### Claude Information

- **GET** `/api/claude/info` - Get Claude binary information
- **GET** `/api/claude/installations` - List all detected Claude installations
- **GET** `/api/claude/version` - Get Claude version

#### Process Management

- **GET** `/api/processes/stats` - Get process statistics
- **POST** `/api/processes/cleanup` - Cleanup completed sessions

#### Server Information

- **GET** `/health` - Health check
- **GET** `/info` - Server information

### WebSocket API

Connect to `ws://localhost:3030/ws` for real-time communication.

#### Client Message Types

##### Start Session
```json
{
  "type": "start_session",
  "data": {
    "project_path": "/path/to/project",
    "prompt": "Your prompt here",
    "model": "claude-3-5-sonnet-20241022"
  }
}
```

##### Cancel Session
```json
{
  "type": "cancel_session",
  "session_id": "session-uuid"
}
```

##### Get Sessions
```json
{
  "type": "get_sessions",
  "active_only": true
}
```

##### Get Output
```json
{
  "type": "get_output",
  "session_id": "session-uuid",
  "lines": 50
}
```

#### Server Message Types

##### Session Started
```json
{
  "type": "session_started",
  "session_id": "session-uuid",
  "message": "Session started successfully"
}
```

##### Session Output
```json
{
  "type": "session_output",
  "session_id": "session-uuid",
  "line": "Output line from Claude",
  "timestamp": "2024-01-01T00:00:00Z"
}
```

##### Session Completed
```json
{
  "type": "session_completed",
  "session_id": "session-uuid",
  "status": "Completed",
  "exit_code": 0
}
```

## Configuration

The server can be configured via a TOML file. By default, it looks for `config.toml` in the data directory (`~/.claudia-server-rust/`).

### Example Configuration

```toml
[server]
max_concurrent_sessions = 10
session_timeout_seconds = 3600
auto_cleanup = true
cleanup_interval_seconds = 300

[claude]
default_model = "claude-3-5-sonnet-20241022"
default_args = [
    "--output-format", "stream-json",
    "--verbose",
    "--dangerously-skip-permissions"
]
verbose = true
skip_permissions = true

[process]
max_output_lines = 1000
save_output_to_disk = true
output_directory = "sessions"

[logging]
level = "info"
log_to_file = true
log_file = "claudia-server.log"
```

## Command Line Options

```
claudia-server [OPTIONS]

OPTIONS:
    -h, --host <HOST>              Host to bind the server to [default: 127.0.0.1]
    -p, --port <PORT>              Port to bind the server to [default: 3030]
        --claude-path <PATH>       Path to Claude CLI binary (auto-detected if not provided)
        --data-dir <DIR>           Directory to store server data [default: ~/.claudia-server-rust]
    -c, --config <FILE>            Configuration file path
        --help                     Print help information
```

## Examples

### JavaScript/Node.js Client

```javascript
// REST API Example
const axios = require('axios');

async function startSession() {
  const response = await axios.post('http://localhost:3030/api/sessions', {
    project_path: '/path/to/project',
    prompt: 'Help me debug this code',
    model: 'claude-3-5-sonnet-20241022'
  });
  
  console.log('Session started:', response.data.session_id);
  return response.data.session_id;
}

// WebSocket Example
const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:3030/ws');

ws.on('open', () => {
  // Start a session
  ws.send(JSON.stringify({
    type: 'start_session',
    data: {
      project_path: '/path/to/project',
      prompt: 'Help me with this code',
      model: 'claude-3-5-sonnet-20241022'
    }
  }));
});

ws.on('message', (data) => {
  const message = JSON.parse(data);
  console.log('Received:', message);
  
  if (message.type === 'session_output') {
    console.log(`[${message.session_id}] ${message.line}`);
  }
});
```

### Python Client

```python
import requests
import websocket
import json

# REST API Example
def start_session():
    response = requests.post('http://localhost:3030/api/sessions', json={
        'project_path': '/path/to/project',
        'prompt': 'Help me write a Python script',
        'model': 'claude-3-5-sonnet-20241022'
    })
    
    result = response.json()
    print(f"Session started: {result['session_id']}")
    return result['session_id']

# WebSocket Example
def on_message(ws, message):
    data = json.loads(message)
    print(f"Received: {data}")
    
    if data['type'] == 'session_output':
        print(f"[{data['session_id']}] {data['line']}")

def on_open(ws):
    # Start a session
    ws.send(json.dumps({
        'type': 'start_session',
        'data': {
            'project_path': '/path/to/project',
            'prompt': 'Help me with this Python code',
            'model': 'claude-3-5-sonnet-20241022'
        }
    }))

ws = websocket.WebSocketApp('ws://localhost:3030/ws',
                           on_message=on_message,
                           on_open=on_open)
ws.run_forever()
```

### Curl Examples

```bash
# Start a new session
curl -X POST http://localhost:3030/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{
    "project_path": "/Users/john/myproject",
    "prompt": "Create a README.md file for this project",
    "model": "claude-3-5-sonnet-20241022"
  }'

# Continue a conversation
curl -X POST http://localhost:3030/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{
    "project_path": "/Users/john/myproject",
    "prompt": "Now add installation instructions",
    "continue_conversation": true
  }'

# Resume a specific session
curl -X POST http://localhost:3030/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{
    "project_path": "/Users/john/myproject",
    "prompt": "Continue working on the documentation",
    "session_id": "existing-session-uuid"
  }'

# Get recent output (last 10 lines)
curl "http://localhost:3030/api/sessions/session-uuid/output?lines=10"

# Get output as plain text
curl "http://localhost:3030/api/sessions/session-uuid/output?format=text"

# List only active sessions
curl "http://localhost:3030/api/sessions?active_only=true"

# Get server info
curl http://localhost:3030/info

# Get Claude installation info
curl http://localhost:3030/api/claude/info

# Get process statistics
curl http://localhost:3030/api/processes/stats
```

## Architecture

The Claudia Server is built with the following components:

- **Axum Web Framework**: HTTP server and routing
- **Tokio**: Async runtime for concurrent session handling
- **Process Manager**: Manages Claude CLI subprocesses
- **Claude Binary Detector**: Auto-discovers Claude installations
- **WebSocket Handler**: Real-time streaming communication
- **Configuration Manager**: TOML-based configuration

### Data Flow

1. Client sends HTTP request or WebSocket message
2. Server validates request and project path
3. Process Manager spawns Claude CLI subprocess
4. Output is captured and streamed to client
5. Session state is tracked and managed
6. Process is cleaned up on completion

## Security Considerations

- The server binds to localhost by default for security
- Project paths are validated to ensure they exist
- Dangerous permissions are skipped by default (configurable)
- Subprocess management includes proper cleanup
- No authentication is built-in (add reverse proxy if needed)

## Troubleshooting

### Claude Binary Not Found

If you get "Claude Code not found" errors:

1. Check if Claude is installed: `which claude`
2. Verify Claude works: `claude --version`
3. Use custom path: `--claude-path /path/to/claude`
4. Check supported installation paths in the documentation

### Sessions Not Starting

1. Verify project path exists and is accessible
2. Check server logs for detailed error messages
3. Ensure Claude binary has proper permissions
4. Test Claude manually in the project directory

### WebSocket Connection Issues

1. Ensure WebSocket endpoint is `ws://localhost:3030/ws`
2. Check firewall settings
3. Verify server is running and accessible
4. Test with a simple WebSocket client first

## Development

### Building from Source

```bash
git clone https://github.com/thedotmack/claudia.git
cd claudia/claudia-server-rust
cargo build --release
```

### Running Tests

```bash
cargo test
```

### Development Mode

```bash
RUST_LOG=debug cargo run -- --host 0.0.0.0 --port 3030
```

This will run the server with debug logging and bind to all interfaces.

## License

This project is licensed under the same license as the main Claudia project.