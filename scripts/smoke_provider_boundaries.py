#!/usr/bin/env python3
"""Real CLI loopback regressions; synthetic secrets only, no external requests."""
import hashlib, http.server, json, os, subprocess, sys, tempfile, threading
from pathlib import Path


def check(binary, mode, field=None, offset=0, reason=None):
    key = 'SYNTHETIC_CREDENTIAL_abcdefghijklmnopqrstuvwxyz_0123456789'
    prompt = 'SYNTHETIC_PRIVATE_REQUEST_abcdefghijklmnopqrstuvwxyz_0123456789'
    packets = []
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_POST(self):
            packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            if mode == 'privacy':
                secret = key if field != 'echo' else prompt
                error = {'code': 'invalid_request', 'message': 'synthetic failure'}
                error['code' if field in ('code', 'echo') else 'message'] = 'Z' * offset + secret
                body = json.dumps({'error': error}).encode(); code = 400; content_type = 'application/json'
            else:
                calls = [{'id': 'must-not-run', 'type': 'function', 'function': {'name': 'write_file', 'arguments': json.dumps({'path': 'sentinel.txt', 'content': 'UNAUTHORIZED_RESULT'})}}]
                code = 200
                if mode == 'stream':
                    calls[0]['index'] = 0
                    event = {'choices': [{'delta': {'content': 'Incomplete fixture', 'tool_calls': calls}, 'finish_reason': reason}]}
                    body = ('data: ' + json.dumps(event) + '\n\ndata: [DONE]\n\n').encode(); content_type = 'text/event-stream'
                else:
                    body = json.dumps({'choices': [{'message': {'role': 'assistant', 'content': 'Incomplete fixture', 'tool_calls': calls}, 'finish_reason': reason}]}).encode(); content_type = 'application/json'
            self.send_response(code); self.send_header('Content-Type', content_type); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-provider-boundaries-') as folder:
            root = Path(folder); workspace = root / 'workspace'; workspace.mkdir(); app = root / 'state'
            sentinel = workspace / 'sentinel.txt'; sentinel.write_text('USER_CONTENT')
            env = {k: v for k, v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(app), BLUE_DRAGON_API_KEY=key, NO_COLOR='1', TERM='dumb')
            cmd = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1', '--model', 'fixture', '--no-rotation']
            if mode != 'stream': cmd += ['--no-stream']
            result = subprocess.run(cmd + ['run', prompt], input='y\n'*10, text=True, capture_output=True, env=env, cwd=workspace, timeout=30)
            assert len(packets) == 1, (mode, 'unexpected repeated provider request', len(packets))
            sessions = list((app / 'sessions').glob('*.json')); assert len(sessions) == 1
            session = json.loads(sessions[0].read_text())
            report = json.dumps(session.get('last_report', {}))
            visible = result.stdout + result.stderr + report
            assert not session.get('observations'), (mode, 'tool executed')
            assert sentinel.read_text() == 'USER_CONTENT', 'workspace changed'
            if mode == 'privacy':
                assert key[:20] not in visible and prompt[:20] not in visible, 'secret prefix leaked in CLI/report'
                assert '400' in visible, 'HTTP status lost'
            else:
                assert session['last_report'].get('finished') is not True, 'incomplete result accepted'
            return {'mode': mode, 'field': field, 'offset': offset, 'stop_reason': reason, 'requests': len(packets), 'native_actions': 0, 'file_unchanged': True, 'status': 'passed'}
    finally:
        server.shutdown(); server.server_close()


def main():
    binary = Path(sys.argv[1]).resolve(); rows = []
    for field in ('code', 'message', 'echo'):
        for offset in (100, 2020): rows.append(check(binary, 'privacy', field, offset))
    for mode in ('stream', 'nonstream'):
        for reason in ('model_context_window_exceeded','pause_turn','refusal','length','content_filter','unknown_future_reason'):
            rows.append(check(binary, mode, reason=reason))
    print(json.dumps({'status': 'passed', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'cases': rows, 'limits': 'Loopback OpenAI-compatible CLI with synthetic credentials; Anthropic covered separately by native transport tests, not CLI or live provider.'}))
if __name__ == '__main__': main()
