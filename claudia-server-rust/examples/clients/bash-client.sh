#!/bin/bash

# Bash client for Claudia Server using curl and WebSocket CLI tools
# Demonstrates basic REST API usage

SERVER_URL="${CLAUDIA_SERVER_URL:-http://localhost:3030}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Check if server is running
check_server() {
    log_info "Checking if Claudia Server is running..."
    
    if curl -s -f "$SERVER_URL/health" > /dev/null; then
        log_success "Server is running at $SERVER_URL"
        return 0
    else
        log_error "Server is not running at $SERVER_URL"
        log_info "Start the server with: claudia-server"
        return 1
    fi
}

# Get server information
get_server_info() {
    log_info "Getting server information..."
    
    response=$(curl -s "$SERVER_URL/info")
    if [ $? -eq 0 ]; then
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to get server info"
        return 1
    fi
}

# Get Claude information
get_claude_info() {
    log_info "Getting Claude binary information..."
    
    response=$(curl -s "$SERVER_URL/api/claude/info")
    if [ $? -eq 0 ]; then
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to get Claude info"
        return 1
    fi
}

# Start a new Claude session
start_session() {
    local project_path="$1"
    local prompt="$2"
    local model="${3:-claude-3-5-sonnet-20241022}"
    
    if [ -z "$project_path" ] || [ -z "$prompt" ]; then
        log_error "Usage: start_session <project_path> <prompt> [model]"
        return 1
    fi
    
    log_info "Starting new session..."
    log_info "Project: $project_path"
    log_info "Prompt: $prompt"
    log_info "Model: $model"
    
    response=$(curl -s -X POST "$SERVER_URL/api/sessions" \
        -H "Content-Type: application/json" \
        -d "{
            \"project_path\": \"$project_path\",
            \"prompt\": \"$prompt\",
            \"model\": \"$model\"
        }")
    
    if [ $? -eq 0 ]; then
        session_id=$(echo "$response" | jq -r '.session_id' 2>/dev/null)
        if [ "$session_id" != "null" ] && [ -n "$session_id" ]; then
            log_success "Session started: $session_id"
            echo "$session_id"
        else
            log_error "Failed to parse session ID from response"
            echo "$response"
            return 1
        fi
    else
        log_error "Failed to start session"
        return 1
    fi
}

# Continue a conversation
continue_session() {
    local project_path="$1"
    local prompt="$2"
    local model="${3:-claude-3-5-sonnet-20241022}"
    
    if [ -z "$project_path" ] || [ -z "$prompt" ]; then
        log_error "Usage: continue_session <project_path> <prompt> [model]"
        return 1
    fi
    
    log_info "Continuing conversation..."
    
    response=$(curl -s -X POST "$SERVER_URL/api/sessions" \
        -H "Content-Type: application/json" \
        -d "{
            \"project_path\": \"$project_path\",
            \"prompt\": \"$prompt\",
            \"model\": \"$model\",
            \"continue_conversation\": true
        }")
    
    if [ $? -eq 0 ]; then
        session_id=$(echo "$response" | jq -r '.session_id' 2>/dev/null)
        if [ "$session_id" != "null" ] && [ -n "$session_id" ]; then
            log_success "Continued session: $session_id"
            echo "$session_id"
        else
            log_error "Failed to parse session ID from response"
            echo "$response"
            return 1
        fi
    else
        log_error "Failed to continue session"
        return 1
    fi
}

# Get session information
get_session() {
    local session_id="$1"
    
    if [ -z "$session_id" ]; then
        log_error "Usage: get_session <session_id>"
        return 1
    fi
    
    log_info "Getting session information..."
    
    response=$(curl -s "$SERVER_URL/api/sessions/$session_id")
    if [ $? -eq 0 ]; then
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to get session information"
        return 1
    fi
}

# Get session output
get_session_output() {
    local session_id="$1"
    local lines="$2"
    local format="${3:-json}"
    
    if [ -z "$session_id" ]; then
        log_error "Usage: get_session_output <session_id> [lines] [format]"
        return 1
    fi
    
    log_info "Getting session output..."
    
    local url="$SERVER_URL/api/sessions/$session_id/output"
    local params=""
    
    if [ -n "$lines" ]; then
        params="?lines=$lines"
    fi
    
    if [ -n "$format" ] && [ "$format" != "json" ]; then
        if [ -n "$params" ]; then
            params="$params&format=$format"
        else
            params="?format=$format"
        fi
    fi
    
    response=$(curl -s "$url$params")
    if [ $? -eq 0 ]; then
        if [ "$format" = "text" ]; then
            echo "$response" | jq -r '.output' 2>/dev/null || echo "$response"
        else
            echo "$response" | jq '.' 2>/dev/null || echo "$response"
        fi
    else
        log_error "Failed to get session output"
        return 1
    fi
}

# List sessions
list_sessions() {
    local active_only="$1"
    
    log_info "Listing sessions..."
    
    local url="$SERVER_URL/api/sessions"
    if [ "$active_only" = "true" ] || [ "$active_only" = "1" ]; then
        url="$url?active_only=true"
    fi
    
    response=$(curl -s "$url")
    if [ $? -eq 0 ]; then
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to list sessions"
        return 1
    fi
}

# Cancel a session
cancel_session() {
    local session_id="$1"
    
    if [ -z "$session_id" ]; then
        log_error "Usage: cancel_session <session_id>"
        return 1
    fi
    
    log_info "Cancelling session: $session_id"
    
    response=$(curl -s -X DELETE "$SERVER_URL/api/sessions/$session_id")
    if [ $? -eq 0 ]; then
        log_success "Session cancelled"
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to cancel session"
        return 1
    fi
}

# Get process statistics
get_stats() {
    log_info "Getting process statistics..."
    
    response=$(curl -s "$SERVER_URL/api/processes/stats")
    if [ $? -eq 0 ]; then
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to get process statistics"
        return 1
    fi
}

# Cleanup completed sessions
cleanup_sessions() {
    log_info "Cleaning up completed sessions..."
    
    response=$(curl -s -X POST "$SERVER_URL/api/processes/cleanup")
    if [ $? -eq 0 ]; then
        log_success "Cleanup completed"
        echo "$response" | jq '.' 2>/dev/null || echo "$response"
    else
        log_error "Failed to cleanup sessions"
        return 1
    fi
}

# Wait for session to complete (polling)
wait_for_session() {
    local session_id="$1"
    local timeout="${2:-300}" # 5 minutes default
    local interval="${3:-2}"  # 2 seconds default
    
    if [ -z "$session_id" ]; then
        log_error "Usage: wait_for_session <session_id> [timeout_seconds] [poll_interval]"
        return 1
    fi
    
    log_info "Waiting for session to complete (timeout: ${timeout}s)..."
    
    local elapsed=0
    while [ $elapsed -lt $timeout ]; do
        local status=$(curl -s "$SERVER_URL/api/sessions/$session_id" | jq -r '.status' 2>/dev/null)
        
        case "$status" in
            "Completed")
                log_success "Session completed successfully"
                return 0
                ;;
            "Failed")
                log_error "Session failed"
                return 1
                ;;
            "Cancelled")
                log_warning "Session was cancelled"
                return 1
                ;;
            "Starting"|"Running")
                echo -n "."
                sleep $interval
                elapsed=$((elapsed + interval))
                ;;
            *)
                log_error "Unknown session status: $status"
                return 1
                ;;
        esac
    done
    
    log_error "Session did not complete within ${timeout} seconds"
    return 1
}

# Print help
show_help() {
    cat << EOF
Claudia Server Bash Client

Usage: $0 <command> [arguments...]

Commands:
    check           - Check if server is running
    info            - Get server information
    claude-info     - Get Claude binary information
    start <path> <prompt> [model]  - Start a new session
    continue <path> <prompt> [model] - Continue conversation
    get <session_id>               - Get session information
    output <session_id> [lines] [format] - Get session output
    list [active]                  - List sessions (active=true for active only)
    cancel <session_id>            - Cancel a session
    stats                          - Get process statistics
    cleanup                        - Cleanup completed sessions
    wait <session_id> [timeout] [interval] - Wait for session to complete

Environment Variables:
    CLAUDIA_SERVER_URL - Server URL (default: http://localhost:3030)

Examples:
    $0 check
    $0 start /path/to/project "Help me write a README"
    $0 continue /path/to/project "Add installation instructions"
    $0 output session-id 10 text
    $0 list active
    $0 wait session-id 600 5

EOF
}

# Main command dispatcher
main() {
    case "$1" in
        "check")
            check_server
            ;;
        "info")
            get_server_info
            ;;
        "claude-info")
            get_claude_info
            ;;
        "start")
            start_session "$2" "$3" "$4"
            ;;
        "continue")
            continue_session "$2" "$3" "$4"
            ;;
        "get")
            get_session "$2"
            ;;
        "output")
            get_session_output "$2" "$3" "$4"
            ;;
        "list")
            list_sessions "$2"
            ;;
        "cancel")
            cancel_session "$2"
            ;;
        "stats")
            get_stats
            ;;
        "cleanup")
            cleanup_sessions
            ;;
        "wait")
            wait_for_session "$2" "$3" "$4"
            ;;
        "help"|"-h"|"--help"|"")
            show_help
            ;;
        *)
            log_error "Unknown command: $1"
            echo
            show_help
            exit 1
            ;;
    esac
}

# Check dependencies
check_deps() {
    if ! command -v curl >/dev/null 2>&1; then
        log_error "curl is required but not installed"
        exit 1
    fi
    
    if ! command -v jq >/dev/null 2>&1; then
        log_warning "jq is not installed - JSON output will not be formatted"
    fi
}

# Run main function
check_deps
main "$@"