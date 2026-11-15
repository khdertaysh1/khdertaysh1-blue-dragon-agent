#!/usr/bin/env python3
"""Resume a synthetic interrupted checkpoint through the real CLI; no job replay."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


def call(name, arguments, ident):
    return {'role': 'assistant', 'content': 'Synthetic fixture', 'tool_calls': [
        {'id': ident, 'type': 'function', 'function': {'name': name, 'arguments': json.dumps(arguments)}}]}


def main(binary):
    packets = []
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            message = call('task_continuity', {}, 'inspect') if len(packets) == 1 else {
                'role': 'assistant', 'content': 'Previous effects need inspection; nothing was restarted.'}
            body = json.dumps({'choices': [{'message': message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-continuity-') as temp:
            root = Path(temp)
            workspace = root / 'workspace'
            workspace.mkdir()
            app = root / 'state'
            sessions = app / 'sessions'
            sessions.mkdir(parents=True)
            job_id = '0123456789abcdef0123456789abcdef'
            args = {'program': sys.executable, 'args': ['-c', 'open("REPLAYED", "w").write("unexpected")']}
            session = {'id': 'continuity-fixture', 'workspace': str(workspace.resolve()),
                       'model': 'fixture', 'updated_at': '2026-01-01T00:00:00Z',
                       'messages': [{'role': 'system', 'content': 'Synthetic fixture'},
                                    {'role': 'user', 'content': 'Never replay previous commands.'},
                                    call('start_job', args, 'job'),
                                    {'role': 'tool', 'tool_call_id': 'job', 'content': json.dumps({'ok': True, 'job_id': job_id, 'state': 'running'})},
                                    call('run_command', args, 'interrupted')],
                       'observations': [{'id': 'E000003', 'message_index': 3, 'tool': 'start_job', 'ok': True, 'excerpt': 'Historical fixture'}],
                       'plan': {'steps': [{'title': 'Inspect uncertain outcome', 'status': 'in_progress'}]},
                       'last_report': {'finished': True}}
            path = sessions / 'continuity-fixture.json'
            path.write_text(json.dumps(session))
            env = {k: v for k, v in os.environ.items() if k.upper() in
                   {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}}
            env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(app),
                       XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'),
                       TERM='dumb', NO_COLOR='1')
            command = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1',
                       '--model', 'fixture', '--no-stream', '--no-rotation', '--resume', 'continuity-fixture',
                       'run', 'Inspect task continuity only. Do not execute old operations.']
            completed = subprocess.run(command, env=env, cwd=workspace, input='', text=True,
                                       capture_output=True, timeout=30)
            assert completed.returncode == 0, completed.stderr[-3000:]
            saved = json.loads(path.read_text())
            results = [json.loads(m['content']) for m in saved['messages'] if m['role'] == 'tool']
            report = next(r for r in results if 'unknown_outcomes' in r)
            assert report['unknown_outcomes'][0]['message_index'] == 5, report
            assert report['historical_jobs'][0]['runtime_status'] == 'not_present_in_current_runtime'
            assert report['current_runtime_jobs'] == []
            assert report['open_plan_steps'] == 1
            assert report['task_completion_proven'] is False and report['actions_replayed'] == 0
            assert not (workspace / 'REPLAYED').exists()
            assert 'APPROVAL REQUIRED' not in completed.stdout
            assert len(packets) == 2, len(packets)
            print(json.dumps({'status': 'passed', 'replayed_commands': 0, 'live_jobs': 0,
                              'unknown_message_index': 5, 'model_requests': 2,
                              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    main(parser.parse_args().binary.resolve())
