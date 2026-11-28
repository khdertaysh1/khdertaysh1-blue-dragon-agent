"""Offline CLI regression for keyless defaults and saved model selection."""
import http.server
import json
import threading
import os
from pathlib import Path
import subprocess
import tempfile
import time

binary = Path(os.environ.get("BLUE_DRAGON_BINARY", "target/release/blue-dragon" + (".exe" if os.name == "nt" else ""))).resolve()
with tempfile.TemporaryDirectory() as home:
    env = {k: v for k, v in os.environ.items() if not k.startswith(
        ("BLUE_DRAGON_", "OPENAI_", "OPENROUTER_", "ANTHROPIC_"))}
    env["BLUE_DRAGON_HOME"] = home

    def run(*args, ok=True):
        result = subprocess.run([str(binary), *args], env=env, capture_output=True,
                                text=True, timeout=10)
        assert (result.returncode == 0) == ok, result.stderr
        return result.stdout + result.stderr

    # Offline catalog fixture: no metadata or model requests during smoke tests.
    (Path(home) / "kilo-models.json").write_text(json.dumps({"fetched_at": int(time.time()), "choices": [
        {"id": "kilo-auto/free", "name": "Auto Free", "context_length": 256000},
        {"id": "liquid/lfm-3-1.7b:free", "name": "LFM3 free", "context_length": 262144}
    ]}))
    assert "kilo-auto/free" in run("doctor")
    assert "liquid/lfm-3-1.7b:free" in run("models")
    run("model", "ollama", "fixture-model")
    assert "fixture-model" in run("doctor")
    run("model", "free")
    saved = json.loads((Path(home) / "model.json").read_text())
    assert saved == {"provider": "kilo", "model": "kilo-auto/free"}
    assert "kilo-auto/free" in run("doctor")
    assert "kilo-auto/free" in run("--provider", "free", "doctor")
    run("model", "free", "paid", ok=False)
    assert json.loads((Path(home) / "model.json").read_text()) == saved
    # An unrelated exported key must never get forwarded to the keyless service.
    env["OPENAI_API_KEY"] = "synthetic-test-key-never-sent"
    assert "API key  : not configured" in run("--provider", "free", "doctor")
    assert "API key  : configured" in run("--provider", "openai", "--model", "fixture", "doctor")
    assert "API key  : not configured" in run("--base-url","http://127.0.0.1:1234/v1","doctor")
    env["OPENROUTER_API_KEY_2"] = "synthetic-second-slot"
    run("--provider", "openrouter", "--model", "fixture", "doctor", ok=False)
    result = subprocess.run([str(binary),"--provider","free","chat"],text=True,capture_output=True,env=env,
                            input="/model openrouter fixture 2\n/exit\n",timeout=10)
    selected = json.loads((Path(home)/"model.json").read_text())
    assert selected == {"provider":"openrouter","model":"fixture","key_slot":2}, result.stdout
    assert "API key  : configured" in run("doctor")
    received = []
    class CompatibleServer(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            received.append((self.path, self.headers.get("Authorization"), self.headers.get("x-api-key"), request["model"]))
            body = ({"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"} if self.path.endswith("/messages")
                    else {"choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]})
            data=json.dumps(body).encode()
            self.send_response(200)
            self.send_header("Content-Type","application/json")
            self.send_header("Content-Length",str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    with http.server.ThreadingHTTPServer(("127.0.0.1",0), CompatibleServer) as server:
        base = f"http://127.0.0.1:{server.server_port}/v1"
        threading.Thread(target=server.serve_forever,daemon=True).start()
        env.update(BLUE_DRAGON_KEY_ENDPOINT=base,BLUE_DRAGON_API_KEY="fixture-custom")
        for protocol in ("chat","messages"):
            if protocol == "messages":
                env["BLUE_DRAGON_ANTHROPIC_ENDPOINT"]=base
            run("--root",home,"--base-url",base,"--model","fixture","--no-stream","run","Say ok.")
        server.shutdown()
    assert received == [("/v1/chat/completions","Bearer fixture-custom",None,"fixture"),
                        ("/v1/messages",None,"fixture-custom","fixture")], received
    # A v0.4.0-alpha.2 saved JarvisClaw destination must not be silently migrated to Kilo.
    (Path(home) / "model.json").write_text(json.dumps({"provider": "free", "model": "auto/free"}))
    assert "jarvisclaw.ai" in run("doctor")
    print("Offline keyless default, saved selection, paid rejection and key isolation passed")
