"""A minimal stdio MCP server with one tool, `echo`: the positive control for MCP tool
calls in the codex detector (confirmation-3 §0.4). Newline-delimited JSON-RPC 2.0."""
import json
import sys

TOOLS = [{"name": "echo", "description": "Echo back the given text.",
          "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}]


def reply(id_, result=None, error=None):
    msg = {"jsonrpc": "2.0", "id": id_}
    msg.update({"error": error} if error else {"result": result})
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    if not line.strip():
        continue
    req = json.loads(line)
    method, id_ = req.get("method"), req.get("id")
    if id_ is None:
        continue                                   # notifications
    if method == "initialize":
        reply(id_, {"protocolVersion": req.get("params", {}).get("protocolVersion", "2025-06-18"),
                    "capabilities": {"tools": {}}, "serverInfo": {"name": "probe", "version": "0.1"}})
    elif method == "tools/list":
        reply(id_, {"tools": TOOLS})
    elif method == "tools/call":
        text = (req.get("params", {}).get("arguments") or {}).get("text", "")
        reply(id_, {"content": [{"type": "text", "text": text}], "isError": False})
    elif method == "ping":
        reply(id_, {})
    else:
        reply(id_, error={"code": -32601, "message": "method not found"})
