#!/usr/bin/env python3
"""Real CLI regression: phase transitions may repeat; unchanged discovery may not."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def main(binary, regression=False):
    packets = []
    names = ['browser', 'file_outline', 'browser', 'file_outline', 'file_outline']
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            index = len(packets)
            packets.append(packet)
            if index < 6:
                name = 'discover_tools' if index < 5 else 'file_outline'
                args = {'names': [names[index]]} if index < 5 else {'path': 'demo.rs'}
                message = {'role': 'assistant', 'content': '', 'tool_calls': [{'id': f'c{index}', 'type': 'function', 'function': {'name': name, 'arguments': json.dumps(args)}}]}
            else:
                message = {'role': 'assistant', 'content': 'Finished the scripted phase sequence.'}
            body = json.dumps({'choices': [{'message': message, 'finish_reason': 'tool_calls' if index < 6 else 'stop'}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-discovery-') as folder:
            root = Path(folder)
            workspace = root / 'workspace'
            workspace.mkdir()
            source = workspace / 'demo.rs'
            source.write_text('fn cycle_observed() {}\n')
            original = source.read_bytes()
            env = {k: v for k, v in os.environ.items() if k in ('PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT')}
            env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(root/'state'), BLUE_DRAGON_INPUT_BUDGET='8192', NO_COLOR='1', TERM='dumb')
            args = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1', '--model', 'fixture', '--no-stream', '--no-rotation', '--read-only', '--max-steps', '8', 'run', 'Inspect only the local Rust file without changing it.']
            process = subprocess.run(args, env=env, capture_output=True, text=True, timeout=45)
            session = json.loads(next((root/'state/sessions').glob('*.json')).read_text())
            results = {m['tool_call_id']: json.loads(m['content']) for m in session['messages'] if m['role'] == 'tool'}
            assert source.read_bytes() == original
            assert len(packets) == 7, (len(packets), process.stderr)
            if regression:
                assert results['c3'].get('repetition_blocked') is True, results['c3']
            else:
                for index, name in enumerate(names[:4]):
                    result = results[f'c{index}']
                    assert result['ok'] is True and result['permission_changed'] is False, result
                    advertised = {d['function']['name'] for d in packets[index+1]['tools']}
                    assert name in advertised
                    assert ('file_outline' if name == 'browser' else 'browser') not in advertised
                assert results['c4']['repetition_blocked'] is True
                assert results['c4']['execution_status'] == 'not_executed'
                observation = next(o for o in session['observations'] if o['tool'] == 'file_outline')
                assert observation['ok'] is True
                assert 'cycle_observed' in json.dumps(results['c5'])
                assert process.returncode == 0, process.stderr
            # The ordinary turn step limit still bounds alternating transitions.
            packets.clear()
            limited = args.copy()
            limited[limited.index('--max-steps')+1] = '3'
            subprocess.run(limited, env=env, capture_output=True, text=True, timeout=45)
            assert len(packets) == 3, len(packets)
            print(json.dumps({'status': 'regression_reproduced' if regression else 'passed', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'cycle': names[:4], 'unchanged_repeat_blocked': not regression, 'step_limited_requests': 3, 'file_unchanged': True, 'limits': 'Scripted loopback provider, native discovery and Rust outline; no real browser or live provider capacity claim.'}))
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--expect-regression', action='store_true')
    args = parser.parse_args()
    main(args.binary.resolve(), args.expect_regression)
