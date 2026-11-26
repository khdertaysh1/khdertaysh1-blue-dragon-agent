"""Exercise the ACP process, optionally through headless Neovim's actual job channel."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def main(binary, editor, plugin):
    errors = []
    calls = 0

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            nonlocal calls
            try:
                packet = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                messages = packet["messages"]
                start = max(i for i, m in enumerate(messages) if m["role"] == "user")
                editing = "replace" in messages[start]["content"].lower()
                results = [json.loads(m["content"]) for m in messages[start:] if m["role"] == "tool"]
                names = [d["function"]["name"] for d in packet.get("tools", [])]
                name = "write_file" if editing else "code_search"
                args = ({"path": "public/assets/bench.js", "content": "unexpected"}
                        if editing else {"kind": "bindings", "query": "formatQuote"})
                if any(r.get("kind") == "bindings" for r in results):
                    result = next(r for r in results if r.get("kind") == "bindings")
                    assert any(r["path"] == "public/assets/bench.js" and r["line"] == 16
                               for r in result["matches"]), result
                    msg = {"content": "Call at public/assets/bench.js:16", "role": "assistant"}
                elif editing and any(r.get("denied") or r.get("ok") is False for r in results):
                    msg = {"content": "Edit declined.", "role": "assistant"}
                else:
                    if name not in names:
                        assert "discover_tools" in names, names
                        args, name = {"names": [name]}, "discover_tools"
                    msg = {"role": "assistant", "tool_calls": [{"id": f"call{calls}", "type": "function",
                           "function": {"name": name, "arguments": json.dumps(args)}}]}
                calls += 1
                payload = json.dumps({"choices": [{"message": msg}]}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
            except Exception as exc:
                errors.append(repr(exc))
                self.send_error(500)

    with tempfile.TemporaryDirectory() as temp, http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider) as server:
        workspace = Path(temp) / "project"
        assets = workspace / "public/assets"
        assets.mkdir(parents=True)
        (assets / "format.js").write_text("export function formatQuote(n){ return String(n); }")
        bench = assets / "bench.js"
        bench.write_text('import {formatQuote} from "./format.js";\n' + "\n"*14 + "formatQuote(3);\n")
        original = bench.read_bytes()
        threading.Thread(target=server.serve_forever, daemon=True).start()
        cmd = [str(binary), "--no-stream", "--no-rotation", "--base-url",
               f"http://127.0.0.1:{server.server_port}/v1", "--model", "fixture", "acp"]
        env = dict(os.environ, BLUE_DRAGON_HOME=str(Path(temp) / "state"))
        if editor:
            env["ACP_TEST_ARGV"] = json.dumps(cmd)
            env["ACP_TEST_PLUGIN"] = str(plugin.resolve())
            scenario = Path(__file__).with_name("smoke_acp_editor.lua").resolve()
            r = subprocess.run([editor, "--headless", "-u", "NONE", "-l", str(scenario)], cwd=workspace,
                               env=env, capture_output=True, text=True, timeout=60)
            assert not errors and r.returncode == 0, (errors, r.stderr)
        else:
            with subprocess.Popen(cmd, env=env, cwd=workspace, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, text=True) as proc:
                def send(method, params, ident=None):
                    p = dict(jsonrpc="2.0", method=method, params=params)
                    if ident is not None:
                        p["id"] = ident
                    proc.stdin.write(json.dumps(p)+"\n")
                    proc.stdin.flush()

                def receive():
                    return json.loads(proc.stdout.readline())

                send("session/new", {"cwd": str(workspace), "mcpServers": []}, 0)
                assert receive()["error"]["code"] == -32002
                send("initialize", {"protocolVersion": 1}, 1)
                assert receive()["result"]["protocolVersion"] == 1
                send("session/new", {"cwd": str(workspace), "mcpServers": []}, 2)
                sid = receive()["result"]["sessionId"]
                def request(i, text):
                    send("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": text}]}, i)
                send("session/prompt", {"sessionId": sid, "prompt": [{"type": "image", "data": "ignored"}]}, 20)
                assert receive()["error"]["code"] == -32602
                request(3, "Where is formatQuote defined and called?")
                messages = []
                while True:
                    packet = receive()
                    if packet.get("id") == 3:
                        assert packet["result"]["stopReason"] == "end_turn"
                        break
                    messages.append(packet)
                assert "bench.js:16" in json.dumps(messages)
                request(4, "Replace public/assets/bench.js with unexpected.")
                while True:
                    packet = receive()
                    if packet.get("method") == "session/request_permission":
                        send("session/cancel", {"sessionId": sid})
                    if packet.get("id") == 4:
                        assert packet["result"]["stopReason"] == "cancelled"
                        break
                proc.stdin.close()
                assert proc.wait(5) == 0
        assert not errors and bench.read_bytes() == original, errors
        print("ACP check passed", editor or "stdio cancellation", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--editor", help="Neovim 0.11+ executable")
    parser.add_argument("--plugin", type=Path, help="Existing agentic.nvim checkout for the editor test")
    args = parser.parse_args()
    if bool(args.editor) != bool(args.plugin):
        parser.error("--editor and --plugin must be used together")
    main(args.binary.resolve(), args.editor, args.plugin)
