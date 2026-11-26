"""Loopback MCP fixture with a request journal and one deliberately lost reply."""
import json
import socket
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class Server(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        message = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        method = message["method"]
        params = message.get("params", {})
        with Path(sys.argv[1]).open("a", encoding="utf-8") as journal:
            journal.write(method + "\n")
        if method != "initialize" and (self.headers.get("Mcp-Session-Id") != "local-test"
                or self.headers.get("MCP-Protocol-Version") != "2025-06-18"):
            self.send_error(400)
            return
        if method == "notifications/initialized":
            self.send_response(202)
            self.end_headers()
            return
        if method == "initialize":
            result = {"protocolVersion": "2025-06-18", "capabilities": {"prompts": {}}}
            if self.path != "/prompts-only":
                result["capabilities"].update(tools={}, resources={})
        elif method == "tools/list":
            result = {"tools": [{"name": "echo", "inputSchema": {"type": "object"}}]}
        elif method == "tools/call":
            if params["arguments"].get("drop"):
                self.connection.shutdown(socket.SHUT_RDWR)
                self.connection.close()
                return
            result = {"content": [{"type": "text", "text": "called-once"}]}
        elif method == "resources/list":
            result = {"resources": [{"uri": "docs://local", "name": "local"}]}
        elif method == "resources/read":
            result = {"contents": [{"uri": params["uri"], "text": "محلي"}]}
        elif method == "prompts/list":
            result = {"prompts": [{"name": "lesson", "arguments": [{"name": "topic", "required": True}]}],
                      "nextCursor": "p2"} if not params else {"prompts": []}
        elif method == "prompts/get":
            result = {"messages": [{"role": "user", "content": {"type": "text",
                      "text": params["arguments"]["topic"]}}]}
        else:
            self.send_error(400)
            return
        reply = json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}, ensure_ascii=False)
        self.send_response(200)
        if method == "initialize":
            self.send_header("Mcp-Session-Id", "local-test")
        if method == "tools/call":
            self.send_header("Content-Type", "text/event-stream")
            payload = (': heartbeat\r\n\r\ndata: {"jsonrpc":"2.0","method":"notifications/progress"}\n\n'
                       + "event: message\ndata: " + reply + "\n\n").encode()
        else:
            self.send_header("Content-Type", "application/json; charset=utf-8")
            payload = reply.encode()
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


with ThreadingHTTPServer(("127.0.0.1", 0), Server) as server:
    print(server.server_port, flush=True)
    server.serve_forever()
