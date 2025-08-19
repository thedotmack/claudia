#!/usr/bin/env python3

"""
Simple Python client for Claudia Server
Demonstrates both REST API and WebSocket usage
"""

import asyncio
import json
import sys
import os
from pathlib import Path
from typing import Optional, List, Dict, Any

import aiohttp
import websockets

SERVER_URL = "http://localhost:3030"
WS_URL = "ws://localhost:3030/ws"


class ClaudiaClient:
    def __init__(self, server_url: str = SERVER_URL):
        self.server_url = server_url
        self.ws_url = WS_URL

    async def start_session(
        self, 
        project_path: str, 
        prompt: str, 
        model: str = "claude-3-5-sonnet-20241022"
    ) -> str:
        """Start a new Claude session via REST API"""
        async with aiohttp.ClientSession() as session:
            try:
                async with session.post(
                    f"{self.server_url}/api/sessions",
                    json={
                        "project_path": project_path,
                        "prompt": prompt,
                        "model": model
                    }
                ) as response:
                    if response.status == 200:
                        data = await response.json()
                        print(f"✅ Session started: {data['session_id']}")
                        return data["session_id"]
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to start session: {error}")
                raise

    async def continue_session(
        self, 
        project_path: str, 
        prompt: str, 
        model: str = "claude-3-5-sonnet-20241022"
    ) -> str:
        """Continue an existing conversation"""
        async with aiohttp.ClientSession() as session:
            try:
                async with session.post(
                    f"{self.server_url}/api/sessions",
                    json={
                        "project_path": project_path,
                        "prompt": prompt,
                        "model": model,
                        "continue_conversation": True
                    }
                ) as response:
                    if response.status == 200:
                        data = await response.json()
                        print(f"✅ Continued session: {data['session_id']}")
                        return data["session_id"]
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to continue session: {error}")
                raise

    async def get_session(self, session_id: str) -> Dict[str, Any]:
        """Get session information"""
        async with aiohttp.ClientSession() as session:
            try:
                async with session.get(
                    f"{self.server_url}/api/sessions/{session_id}"
                ) as response:
                    if response.status == 200:
                        return await response.json()
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to get session: {error}")
                raise

    async def get_session_output(
        self, 
        session_id: str, 
        lines: Optional[int] = None, 
        format: str = "json"
    ) -> Dict[str, Any]:
        """Get session output"""
        async with aiohttp.ClientSession() as session:
            try:
                params = {}
                if lines:
                    params["lines"] = lines
                if format:
                    params["format"] = format

                async with session.get(
                    f"{self.server_url}/api/sessions/{session_id}/output",
                    params=params
                ) as response:
                    if response.status == 200:
                        return await response.json()
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to get session output: {error}")
                raise

    async def list_sessions(self, active_only: bool = False) -> List[Dict[str, Any]]:
        """List all sessions"""
        async with aiohttp.ClientSession() as session:
            try:
                params = {}
                if active_only:
                    params["active_only"] = "true"

                async with session.get(
                    f"{self.server_url}/api/sessions",
                    params=params
                ) as response:
                    if response.status == 200:
                        data = await response.json()
                        return data["sessions"]
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to list sessions: {error}")
                raise

    async def cancel_session(self, session_id: str) -> Dict[str, Any]:
        """Cancel a session"""
        async with aiohttp.ClientSession() as session:
            try:
                async with session.delete(
                    f"{self.server_url}/api/sessions/{session_id}"
                ) as response:
                    if response.status == 200:
                        data = await response.json()
                        print(f"✅ Session cancelled: {data['message']}")
                        return data
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to cancel session: {error}")
                raise

    async def get_server_info(self) -> Dict[str, Any]:
        """Get server info"""
        async with aiohttp.ClientSession() as session:
            try:
                async with session.get(f"{self.server_url}/info") as response:
                    if response.status == 200:
                        return await response.json()
                    else:
                        error_text = await response.text()
                        raise Exception(f"HTTP {response.status}: {error_text}")
            except Exception as error:
                print(f"❌ Failed to get server info: {error}")
                raise

    async def start_streaming_session(
        self, 
        project_path: str, 
        prompt: str, 
        model: str = "claude-3-5-sonnet-20241022"
    ) -> Optional[str]:
        """Start a streaming session via WebSocket"""
        session_id = None
        
        try:
            async with websockets.connect(self.ws_url) as websocket:
                print("🔌 WebSocket connected")
                
                # Start session
                await websocket.send(json.dumps({
                    "type": "start_session",
                    "data": {
                        "project_path": project_path,
                        "prompt": prompt,
                        "model": model
                    }
                }))
                
                async for message_data in websocket:
                    try:
                        message = json.loads(message_data)
                        
                        if message["type"] == "session_started":
                            session_id = message["session_id"]
                            print(f"✅ Streaming session started: {session_id}")
                            
                        elif message["type"] == "session_output":
                            print(f"📝 [{message['session_id']}] {message['line']}")
                            
                        elif message["type"] == "session_completed":
                            print(f"✅ Session completed: {message['status']} (exit code: {message.get('exit_code')})")
                            break
                            
                        elif message["type"] == "session_cancelled":
                            print("❌ Session was cancelled")
                            break
                            
                        elif message["type"] == "error":
                            print(f"❌ Server error: {message['message']}")
                            break
                            
                        else:
                            print(f"📨 Received: {message}")
                            
                    except json.JSONDecodeError as error:
                        print(f"❌ Failed to parse message: {error}")
                        
        except Exception as error:
            print(f"❌ WebSocket error: {error}")
            raise
            
        finally:
            print("🔌 WebSocket disconnected")
            
        return session_id


async def main():
    """Example usage"""
    client = ClaudiaClient()

    try:
        # Get server info
        print("📊 Server Info:")
        info = await client.get_server_info()
        print(json.dumps(info, indent=2))
        print()

        # Example project path (use command line argument or current directory)
        project_path = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
        print(f"🗂️  Using project path: {project_path}")
        print()

        # Start a session via REST API
        print("🚀 Starting REST API session...")
        session_id = await client.start_session(
            project_path,
            "Help me create a simple Python Flask web server"
        )

        # Wait a bit for the session to process
        await asyncio.sleep(2)

        # Get session info
        print("📋 Session Info:")
        session_info = await client.get_session(session_id)
        print(json.dumps(session_info, indent=2))
        print()

        # Get session output
        print("📝 Session Output:")
        output = await client.get_session_output(session_id, lines=10)
        print(json.dumps(output, indent=2))
        print()

        # List all sessions
        print("📋 All Sessions:")
        sessions = await client.list_sessions()
        for session in sessions:
            prompt_preview = session["prompt"][:50] + "..." if len(session["prompt"]) > 50 else session["prompt"]
            print(f"- {session['id']} ({session['status']}) - {prompt_preview}")
        print()

        # Example of streaming session (uncomment to try)
        """
        print("🚀 Starting streaming session...")
        await client.start_streaming_session(
            project_path,
            "Help me write a simple Python script to process JSON files"
        )
        """

    except Exception as error:
        print(f"❌ Error: {error}")
        sys.exit(1)


if __name__ == "__main__":
    # Install dependencies: pip install aiohttp websockets
    try:
        import aiohttp
        import websockets
    except ImportError:
        print("❌ Missing dependencies. Install with:")
        print("pip install aiohttp websockets")
        sys.exit(1)
    
    asyncio.run(main())