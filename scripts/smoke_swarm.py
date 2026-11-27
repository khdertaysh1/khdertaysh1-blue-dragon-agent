"""Exercise the interactive coordinator with a local deterministic provider."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


def run(binary, conflict=False):
    barrier = threading.Barrier(3)
    errors = []
    original = {"calc.py": "def double(x): return x\n"}
    changed = {"calc.py": "def double(x): return 2*x\n",
               "check.py": "from calc import double\nassert double(3) == 6\n"}
    plan = [{"task": name, "kind": kind, "after": deps} for name, kind, deps in
            [("fix multiplication", "implementation", []), ("check multiplication", "test", []),
             ("review inputs", "review", []), ("summarize inputs", "review", [2])]]

    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                messages = request["messages"]
                answer = {"role": "assistant", "content": "review complete"}
                if "coordinate local coding" in messages[0]["content"]:
                    answer["content"] = json.dumps(plan)
                else:
                    task = messages[1]["content"]
                    path = next((p for tag, p in [("fix multiplication", "calc.py"),
                                                  ("check multiplication", "check.py")] if tag in task), None)
                    if not any(m["role"] == "tool" for m in messages):
                        if "summarize inputs" in task:
                            assert "review complete" in task, task
                        else:
                            barrier.wait(12)
                        if path:
                            args = {"files": [{"path": path, "content": changed[path],
                                               "expected_sha256": (hashlib.sha256(original[path].encode()).hexdigest() if path in original else "absent")}]}
                            answer["tool_calls"] = [{"type": "function", "id": "change", "function": {
                                "name": "agent_propose_edits", "arguments": json.dumps(args)}}]
                data = json.dumps({"choices": [{"message": answer}]}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except Exception as error:
                errors.append(repr(error))
                self.send_error(500)

    with tempfile.TemporaryDirectory() as temp, http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model) as server:
        root = Path(temp)
        project = root / "repo"
        project.mkdir()
        def git(*args):
            return subprocess.check_output(["git", *args], cwd=project, text=True)
        git("init", "-q")
        for p, value in original.items():
            (project / p).write_bytes(value.encode())
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", "commit", "-qm", "base")
        if conflict:
            (project / "check.py").write_bytes(b"# local edit\n")
        threading.Thread(target=server.serve_forever, daemon=True).start()
        command = json.dumps([sys.executable, "check.py"])
        # One planner, four delegates, two trial checks,
        # one combined workspace patch and one final workspace check.
        approvals = 7 if conflict else 9
        proc = subprocess.run([binary, "--root", str(project), "--base-url",
                               f"http://127.0.0.1:{server.server_port}/v1", "--model", "fixture",
                               "--no-stream", "--no-rotation", "chat"],
                              input=f"/swarm fix . | {command} | Fix multiplication\n" + "y\n"*approvals + "/exit\n",
                              capture_output=True, text=True, timeout=90,
                              env=dict(os.environ, BLUE_DRAGON_HOME=str(root / "state"), BLUE_DRAGON_SWARM_LIVE="3"))
        server.shutdown()
        assert not errors, errors
        output = proc.stdout + proc.stderr
        assert proc.returncode == 0 and '"phase": "implementation"' in output, output[-5000:]
        if not conflict:
            assert output.count('"phase": "workspace_apply"') == 1, output[-5000:]
            assert '"isolated_checkout_updated": true' in str(output).replace('\\"','"') or '"trial_checkout_updated": true' in output
        assert git("worktree", "list", "--porcelain").count("worktree ") == 1
        if conflict:
            assert '"verified": false' in output, output[-5000:]
            assert (project / "check.py").read_bytes() == b"# local edit\n"
            assert (project / "calc.py").read_bytes() == original["calc.py"].encode()
        else:
            assert '"state": "verified"' in output and '"command_outcome": "nonzero_exit"' in output, output[-5000:]
            for p, value in changed.items():
                assert (project / p).read_bytes() == value.encode()


def default_capacity(binary):
    class Server(http.server.ThreadingHTTPServer):
        request_queue_size = 64

    count = 32
    barrier = threading.Barrier(count)
    seen = set()
    lock = threading.Lock()

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            messages = json.loads(self.rfile.read(int(self.headers["Content-Length"])))["messages"]
            if "coordinate local coding" in messages[0]["content"]:
                content = json.dumps([{"task": f"Review local unit {i}", "kind": "review", "after": []}
                                      for i in range(count)])
            elif "independent read-only research delegate" in messages[0]["content"]:
                with lock:
                    seen.add(messages[1]["content"])
                try:
                    barrier.wait(35)
                except threading.BrokenBarrierError:
                    self.send_error(500)
                    return
                content = "Reviewed"
            else:
                self.send_error(400)
                return
            data = json.dumps({"choices": [{"message": {"role": "assistant", "content": content}}]}).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    with tempfile.TemporaryDirectory() as tmp, Server(("127.0.0.1", 0), Provider) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        env = dict(os.environ, BLUE_DRAGON_HOME=str(Path(tmp) / "state"))
        env.pop("BLUE_DRAGON_SWARM_LIVE", None)
        proc = subprocess.run([binary, "--root", tmp, "--base-url", f"http://127.0.0.1:{server.server_port}/v1",
                               "--model", "fixture", "--no-stream", "--no-rotation", "chat"],
                              input="/swarm auto . | Review independent local units\n" + "y\n"*(count+1) + "/exit\n",
                              capture_output=True, text=True, timeout=90, env=env)
        server.shutdown()
        assert len(seen) == count and '"state": "workers_completed"' in proc.stdout, (len(seen), proc.stdout[-1000:])



def nested_review(binary):
    """One execution slot must suffice for parent -> leaf -> parent."""
    errors = []
    calls = []
    def tool(name, args, index):
        return {"role": "assistant", "tool_calls": [{"id": str(index), "type": "function",
                "function": {"name": name, "arguments": json.dumps(args)}}]}

    class Server(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            try:
                packet = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                messages = packet["messages"]
                tools = {item["function"]["name"] for item in packet.get("tools", [])}
                replies = [json.loads(m["content"]) for m in messages if m["role"] == "tool"]
                n = len(replies)
                answer = {"role": "assistant", "content": ""}
                if "coordinate local coding" in messages[0]["content"]:
                    answer["content"] = json.dumps([{"task": "trace price display", "kind": "review", "after": []}])
                elif "agent_subtask" in tools:
                    calls.append("worker")
                    if n == 0:
                        answer = tool("agent_subtask", {"task": "out of scope", "paths": ["outside.js"]}, n)
                    elif n == 1:
                        assert replies[-1]["ok"] is False
                        answer = tool("agent_subtask", {"task": "locate caller", "paths": ["src/view.js"]}, n)
                    else:
                        assert replies[-1]["state"] == "completed" and replies[-1]["evidence_ids"]
                        answer["content"] = "Display receipt from " + replies[-1]["agent_id"]
                else:
                    calls.append("leaf")
                    assert not {"agent_propose_edits", "agent_message", "run_command", "browser"} & tools
                    if n == 0:
                        answer = tool("agent_subtask", {"task": "too deep", "paths": ["src/view.js"]}, n)
                    elif n == 1:
                        assert replies[-1]["denied"]
                        answer = tool("file_info", {"path": "outside.js"}, n)
                    elif n == 2:
                        assert replies[-1]["denied"]
                        answer = tool("read_file_window", {"path": "src/view.js", "byte_offset": 0, "byte_count": 128}, n)
                    else:
                        assert "formatQuote(total)" in str(replies[-1]), replies[-1]
                        answer["content"] = "src/view.js calls formatQuote(total)"
                data = json.dumps({"choices": [{"message": answer}]}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except Exception as error:
                errors.append(repr(error))
                self.send_error(500)

    with tempfile.TemporaryDirectory() as tmp, http.server.ThreadingHTTPServer(("127.0.0.1", 0), Server) as server:
        project = Path(tmp) / "project"
        (project / "src").mkdir(parents=True)
        (project / "src/view.js").write_text("const text = formatQuote(total);")
        (project / "outside.js").write_text("private fixture")
        threading.Thread(target=server.serve_forever, daemon=True).start()
        state = Path(tmp) / "state"
        proc = subprocess.run([binary, "--root", str(project), "--base-url",
                               f"http://127.0.0.1:{server.server_port}/v1", "--model", "fixture",
                               "--no-stream", "--no-rotation", "chat"],
                              input="/swarm auto src | Trace the price display\ny\ny\n/swarm status\n/exit\n",
                              capture_output=True, text=True, timeout=45,
                              env=dict(os.environ, BLUE_DRAGON_HOME=str(state), BLUE_DRAGON_SWARM_LIVE="1"))
        server.shutdown()
        assert not errors, errors
        assert calls == ["worker", "worker", "leaf", "leaf", "leaf", "leaf", "worker"], (calls, proc.stdout)
        assert '"subtasks": [' in proc.stdout and '"state": "workers_completed"' in proc.stdout
        checkpoints = [json.loads(p.read_text()) for p in state.glob("sessions/swarms/*/*.json") if p.name != "budget.json"]
        assert len(checkpoints) == 2
        leaf = next(c for c in checkpoints if c["last_report"].get("parent_agent_id"))
        parent = next(c for c in checkpoints if c["id"] != leaf["id"])
        assert leaf["last_report"]["parent_agent_id"] == parent["id"]
        assert leaf["last_report"]["state"] == "completed"
        budget = json.loads(next(state.glob("sessions/swarms/*/budget.json")).read_text())
        assert budget["requests"] == 49 - 1 - len(calls) and budget["calls"] == 192 - 5


if __name__ == "__main__":
    executable = str(Path(sys.argv[1]).resolve())
    nested_review(executable)
    default_capacity(executable)
    run(executable)
    run(executable, conflict=True)
    print("32 concurrent workers at default capacity, three concurrent repair workers, dependent fourth worker, isolated red/green repair, and dirty workspace conflict passed.")
