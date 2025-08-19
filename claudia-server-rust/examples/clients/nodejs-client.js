#!/usr/bin/env node

/**
 * Simple Node.js client for Claudia Server
 * Demonstrates both REST API and WebSocket usage
 */

const axios = require('axios');
const WebSocket = require('ws');

const SERVER_URL = 'http://localhost:3030';
const WS_URL = 'ws://localhost:3030/ws';

class ClaudiaClient {
    constructor(serverUrl = SERVER_URL) {
        this.serverUrl = serverUrl;
        this.wsUrl = WS_URL;
    }

    /**
     * Start a new Claude session via REST API
     */
    async startSession(projectPath, prompt, model = 'claude-3-5-sonnet-20241022') {
        try {
            const response = await axios.post(`${this.serverUrl}/api/sessions`, {
                project_path: projectPath,
                prompt: prompt,
                model: model
            });
            
            console.log('✅ Session started:', response.data.session_id);
            return response.data.session_id;
        } catch (error) {
            console.error('❌ Failed to start session:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * Continue an existing conversation
     */
    async continueSession(projectPath, prompt, model = 'claude-3-5-sonnet-20241022') {
        try {
            const response = await axios.post(`${this.serverUrl}/api/sessions`, {
                project_path: projectPath,
                prompt: prompt,
                model: model,
                continue_conversation: true
            });
            
            console.log('✅ Continued session:', response.data.session_id);
            return response.data.session_id;
        } catch (error) {
            console.error('❌ Failed to continue session:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * Get session information
     */
    async getSession(sessionId) {
        try {
            const response = await axios.get(`${this.serverUrl}/api/sessions/${sessionId}`);
            return response.data;
        } catch (error) {
            console.error('❌ Failed to get session:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * Get session output
     */
    async getSessionOutput(sessionId, lines = null, format = 'json') {
        try {
            const params = new URLSearchParams();
            if (lines) params.append('lines', lines);
            if (format) params.append('format', format);
            
            const response = await axios.get(`${this.serverUrl}/api/sessions/${sessionId}/output?${params}`);
            return response.data;
        } catch (error) {
            console.error('❌ Failed to get session output:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * List all sessions
     */
    async listSessions(activeOnly = false) {
        try {
            const params = activeOnly ? '?active_only=true' : '';
            const response = await axios.get(`${this.serverUrl}/api/sessions${params}`);
            return response.data.sessions;
        } catch (error) {
            console.error('❌ Failed to list sessions:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * Cancel a session
     */
    async cancelSession(sessionId) {
        try {
            const response = await axios.delete(`${this.serverUrl}/api/sessions/${sessionId}`);
            console.log('✅ Session cancelled:', response.data.message);
            return response.data;
        } catch (error) {
            console.error('❌ Failed to cancel session:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * Get server info
     */
    async getServerInfo() {
        try {
            const response = await axios.get(`${this.serverUrl}/info`);
            return response.data;
        } catch (error) {
            console.error('❌ Failed to get server info:', error.response?.data || error.message);
            throw error;
        }
    }

    /**
     * Start a streaming session via WebSocket
     */
    async startStreamingSession(projectPath, prompt, model = 'claude-3-5-sonnet-20241022') {
        return new Promise((resolve, reject) => {
            const ws = new WebSocket(this.wsUrl);
            let sessionId = null;

            ws.on('open', () => {
                console.log('🔌 WebSocket connected');
                
                // Start session
                ws.send(JSON.stringify({
                    type: 'start_session',
                    data: {
                        project_path: projectPath,
                        prompt: prompt,
                        model: model
                    }
                }));
            });

            ws.on('message', (data) => {
                try {
                    const message = JSON.parse(data);
                    
                    switch (message.type) {
                        case 'session_started':
                            sessionId = message.session_id;
                            console.log('✅ Streaming session started:', sessionId);
                            break;
                            
                        case 'session_output':
                            console.log(`📝 [${message.session_id}] ${message.line}`);
                            break;
                            
                        case 'session_completed':
                            console.log(`✅ Session completed: ${message.status} (exit code: ${message.exit_code})`);
                            ws.close();
                            resolve(sessionId);
                            break;
                            
                        case 'session_cancelled':
                            console.log('❌ Session was cancelled');
                            ws.close();
                            resolve(sessionId);
                            break;
                            
                        case 'error':
                            console.error('❌ Server error:', message.message);
                            ws.close();
                            reject(new Error(message.message));
                            break;
                            
                        default:
                            console.log('📨 Received:', message);
                    }
                } catch (error) {
                    console.error('❌ Failed to parse message:', error);
                }
            });

            ws.on('error', (error) => {
                console.error('❌ WebSocket error:', error);
                reject(error);
            });

            ws.on('close', () => {
                console.log('🔌 WebSocket disconnected');
            });
        });
    }
}

// Example usage
async function main() {
    const client = new ClaudiaClient();

    try {
        // Get server info
        console.log('📊 Server Info:');
        const info = await client.getServerInfo();
        console.log(JSON.stringify(info, null, 2));
        console.log();

        // Example project path (change this to your actual project)
        const projectPath = process.argv[2] || process.cwd();
        console.log(`🗂️  Using project path: ${projectPath}`);
        console.log();

        // Start a session via REST API
        console.log('🚀 Starting REST API session...');
        const sessionId = await client.startSession(
            projectPath,
            'Help me create a simple Node.js HTTP server'
        );

        // Wait a bit for the session to process
        await new Promise(resolve => setTimeout(resolve, 2000));

        // Get session info
        console.log('📋 Session Info:');
        const sessionInfo = await client.getSession(sessionId);
        console.log(JSON.stringify(sessionInfo, null, 2));
        console.log();

        // Get session output
        console.log('📝 Session Output:');
        const output = await client.getSessionOutput(sessionId, 10);
        console.log(output);
        console.log();

        // List all sessions
        console.log('📋 All Sessions:');
        const sessions = await client.listSessions();
        sessions.forEach(session => {
            console.log(`- ${session.id} (${session.status}) - ${session.prompt.substring(0, 50)}...`);
        });
        console.log();

        // Example of streaming session (uncomment to try)
        /*
        console.log('🚀 Starting streaming session...');
        await client.startStreamingSession(
            projectPath,
            'Help me write a simple Python script to read CSV files'
        );
        */

    } catch (error) {
        console.error('❌ Error:', error.message);
        process.exit(1);
    }
}

// Run example if this file is executed directly
if (require.main === module) {
    main().catch(console.error);
}

module.exports = ClaudiaClient;