"""Offline CLI regression for keyless defaults and saved model selection."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

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

    assert "auto/free" in run("doctor")
    assert "JarvisClaw" in run("models")
    run("model", "ollama", "fixture-model")
    assert "fixture-model" in run("doctor")
    run("model", "free")
    saved = json.loads((Path(home) / "model.json").read_text())
    assert saved == {"provider": "free", "model": "auto/free"}
    assert "auto/free" in run("doctor")
    assert "auto/free" in run("--provider", "free", "doctor")
    run("model", "free", "paid", ok=False)
    assert json.loads((Path(home) / "model.json").read_text()) == saved
    # An unrelated exported key must never get forwarded to the keyless service.
    env["OPENAI_API_KEY"] = "synthetic-test-key-never-sent"
    assert "API key  : not configured" in run("--provider", "free", "doctor")
    print("Offline keyless default, saved selection, paid rejection and key isolation passed")
