"""Local stdio-only integration fixture. Never sends network requests."""
import json
import sys
initialized = False
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "notifications/initialized":
        initialized = True
        continue
    if "id" not in request:
        continue
    if method == "initialize":
        result = {"protocolVersion": "2025-03-26", "capabilities": {"tools": {}}}
    elif method == "tools/list":
        assert initialized, "Client must finish lifecycle before listing"
        result = {"tools": [{"name": "echo", "description": "Echo fixture", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}]}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": request["params"]["arguments"]["text"]}]}
    else:
        raise AssertionError(method)
    identifier = request["id"] + (1 if "--wrong-id" in sys.argv else 0)
    print(json.dumps({"jsonrpc": "2.0", "id": identifier, "result": result}), flush=True)
