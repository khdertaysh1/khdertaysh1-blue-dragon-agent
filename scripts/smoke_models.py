"""Offline CLI regression for keyless defaults and saved model selection."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

binary = Path("target/release/blue-dragon" + (".exe" if os.name == "nt" else "")).resolve()
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
    # A v0.4.0-alpha.2 saved JarvisClaw destination must not be silently migrated to Kilo.
    (Path(home) / "model.json").write_text(json.dumps({"provider": "free", "model": "auto/free"}))
    assert "jarvisclaw.ai" in run("doctor")
    print("Offline keyless default, saved selection, paid rejection and key isolation passed")
