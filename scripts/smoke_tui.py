"""Real Unix PTY smoke test; no model call or credentials needed."""
import json
from pathlib import Path
import fcntl
import os
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time
from pty_helpers import wait_exit, contains_styled_text

master, slave = pty.openpty()
process = None
try:
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    before = termios.tcgetattr(slave)
    with tempfile.TemporaryDirectory() as home:
        (Path(home) / "kilo-models.json").write_text(json.dumps({"fetched_at": int(time.time()), "choices": [
            {"id": "kilo-auto/free", "name": "Auto Free", "context_length": 256000},
            {"id": "liquid/lfm-3-1.7b:free", "name": "LFM3 free", "context_length": 262144}
        ]}))
        env = dict(os.environ, TERM="xterm-256color", BLUE_DRAGON_HOME=home)
        process = subprocess.Popen(
            [os.environ.get("BLUE_DRAGON_BINARY", "./target/release/blue-dragon"), "--read-only", "tui"],
            stdin=slave, stdout=slave, stderr=slave, env=env,
        )
        captured = bytearray()
        def wait_for(text, seconds=5):
            until = time.monotonic() + seconds
            while time.monotonic() < until:
                if contains_styled_text(captured, text):
                    return
                if select.select([master], [], [], 0.05)[0]:
                    captured.extend(os.read(master, 65536))
            raise AssertionError(f"TUI did not render {text!r}; exit={process.poll()}; tail={bytes(captured[-4096:])!r}")
        wait_for(b"BLUE DRAGON")
        os.write(master, b"/toolkit self-test\r")
        wait_for(b"inputs_unchanged")
        os.write(master, b"/help\r")
        wait_for(b"Commands:")
        os.write(master, b"/model free\r")
        until = time.monotonic() + 5
        while not os.path.isfile(os.path.join(home, "model.json")) and time.monotonic() < until:
            if select.select([master], [], [], 0.05)[0]:
                captured.extend(os.read(master, 65536))
        assert os.path.isfile(os.path.join(home, "model.json")), "Model selection was not saved"

        os.write(master, b"/models\r")
        wait_for(b"FREE MODELS")
        # Select a concrete keyless model through the actual terminal modal.
        os.write(master, b"lfm")
        os.write(master, b"\r")
        until = time.monotonic() + 5
        selected = None
        while time.monotonic() < until:
            try:
                selected = json.loads((Path(home) / "model.json").read_text())["model"]
            except (OSError, ValueError, KeyError):
                pass
            if selected == "liquid/lfm-3-1.7b:free":
                break
            if select.select([master], [], [], 0.05)[0]:
                captured.extend(os.read(master, 65536))
        assert selected == "liquid/lfm-3-1.7b:free", selected
        os.write(master, b"/exit\r")
        code, exit_seconds = wait_exit(process, master, captured, seconds=5)
        assert code == 0, f"TUI exit code {code}; tail={bytes(captured[-4096:])!r}"
        assert b"\x1b[?1049l" in captured
        assert termios.tcgetattr(slave) == before
        print(f"PTY model picker/filter/select/save, help, exit and terminal-mode restoration passed; exit={exit_seconds:.3f}s")
finally:
    if process is not None and process.poll() is None:
        process.kill()
        process.wait()
    os.close(master)
    os.close(slave)
