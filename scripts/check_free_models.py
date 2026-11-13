"""Opt-in live keyless model checks in synthetic workspaces (never run by CI)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--model", action="append", help="Kilo free model ID; repeatable")
args = parser.parse_args()
models = args.model or ["kilo-auto/free", "cohere/north-mini-code:free", "liquid/lfm-3-1.7b:free"]
binary = Path("target/release/blue-dragon" + (".exe" if os.name == "nt" else "")).resolve()
failures = []
for model in models:
    with tempfile.TemporaryDirectory() as folder:
        home = Path(folder)
        workspace = home / "workspace"
        workspace.mkdir()
        (workspace / "sample.txt").write_text("BLUE_DRAGON_SYNTHETIC_42\n")
        env = {k: v for k, v in os.environ.items() if not k.startswith(
            ("BLUE_DRAGON_", "OPENAI_", "OPENROUTER_", "ANTHROPIC_", "KILO_"))}
        env["BLUE_DRAGON_HOME"] = str(home / "state")
        start = time.monotonic()
        try:
            result = subprocess.run([
                str(binary), "--root", str(workspace), "--provider", "kilo", "--model", model,
                "--read-only", "--max-steps", "4", "run",
                "Use read_file to read sample.txt, then reply with its exact contents. Do not guess."
            ], env=env, text=True, encoding="utf-8", errors="replace", capture_output=True, timeout=150)
            reports = [json.loads(p.read_text())["last_report"]
                       for p in (home / "state" / "sessions").glob("*.json")]
            ok = result.returncode == 0 and "BLUE_DRAGON_SYNTHETIC_42" in result.stdout and any(
                r.get("finished") and any(a.get("tool") == "read_file" and a.get("ok")
                                          for a in r.get("actions", [])) for r in reports)
            print(json.dumps({"model": model, "ok": ok, "seconds": round(time.monotonic() - start, 1)}))
            if not ok:
                failures.append(model)
                print((result.stderr or result.stdout)[-1000:])
        except subprocess.TimeoutExpired:
            failures.append(model)
            print(json.dumps({"model": model, "ok": False, "error": "deadline exceeded"}))
raise SystemExit(1 if failures else 0)
