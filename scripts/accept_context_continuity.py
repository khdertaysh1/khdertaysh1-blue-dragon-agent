#!/usr/bin/env python3
"""Real CLI, scripted provider: 30 Arabic exchanges, native reads/browser status.
No live model or DOM claim. Runs identical workload/config before and after.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import traceback


def valid_window(result, observation, expected):
    return (
        observation.get('ok') is True
        and result.get('ok') is not False
        and not result.get('denied') and not result.get('error')
        and not result.get('cached_result') and result.get('fresh_evidence') is not False
        and result.get('execution_status', 'completed') in ('completed', 'succeeded', 'success')
        and result.get('byte_offset') == 0 and result.get('end_byte_offset') == len(expected)
        and len(expected) > 0 and result.get('content') == expected.decode()
        and result.get('window_sha256') == hashlib.sha256(expected).hexdigest()
        and result.get('evidence_id') == observation.get('id')
    )


def negative_controls():
    expected = b'fixture'
    observation = {'ok': True, 'id': 'E1'}
    good = {'content': expected.decode(), 'byte_offset': 0, 'end_byte_offset': len(expected),
            'window_sha256': hashlib.sha256(expected).hexdigest(), 'evidence_id': 'E1'}
    assert valid_window(good, observation, expected)
    for status in ('failed', 'denied', 'not_executed', 'cancelled', 'unknown', 'stale'):
        assert not valid_window(dict(good, execution_status=status), observation, expected)
    for change in ({'ok': False}, {'denied': True}, {'error': 'failed'}, {'end_byte_offset': 0},
                   {'cached_result': True}, {'fresh_evidence': False}, {'evidence_id': 'other'},
                   {'window_sha256': '0'*64}):
        assert not valid_window(dict(good, **change), observation, expected)
    assert not valid_window(good, {'ok': False, 'id': 'E1'}, expected)


def run(binary, require_success):
    packets, errors, verified = [], [], []
    with tempfile.TemporaryDirectory(prefix='blue-continuity-') as directory:
        root = Path(directory)
        work = root / 'project'
        work.mkdir()
        fixture = b'export const sentinel = "CONTINUITY_42";\n' + b'// padding\n' * 10000
        (work / 'app.js').write_bytes(fixture)
        state = root / 'state'
        env = {k: v for k, v in os.environ.items() if k in ['PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT']}
        env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(state),
                   BLUE_DRAGON_INPUT_BUDGET='8192', BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048',
                   BLUE_DRAGON_RPM='120', NO_COLOR='1', TERM='dumb')
        phase = 0
        class Server(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass
            def do_POST(self):
                nonlocal phase
                try:
                    packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                    names = {d['function']['name'] for d in packet['tools']}
                    packets.append({'max_output_tokens': packet.get('max_tokens'), 'tool_names': sorted(names),
                                    'message_bytes': len(json.dumps(packet['messages'], ensure_ascii=False).encode())})
                    assert any('NEVER MODIFY FILES' in str(m.get('content', '')) for m in packet['messages'])
                    assert packet.get('max_tokens') == 2048
                    if phase % 2 == 0:
                        assert 'read_file_window' in names
                        # A URL in natural language advertises browser; old version needs discovery.
                        calls = [{'id': f'read-{phase}', 'type': 'function', 'function': {
                            'name': 'read_file_window', 'arguments': json.dumps({'path': 'app.js', 'byte_offset': 0, 'byte_count': 128})}}]
                        if 'browser' in names:
                            calls.append({'id': f'browser-{phase}', 'type': 'function', 'function': {'name': 'browser', 'arguments': '{"action":"status"}'}})
                        message = {'role': 'assistant', 'content': '', 'tool_calls': calls}
                        finish = 'tool_calls'
                    else:
                        session = json.loads(session_path.read_text())
                        reads = [o for o in session['observations'] if o['tool'] == 'read_file_window']
                        assert len(reads) == len(verified) + 1
                        messages = [m for m in session['messages'] if m.get('tool_call_id') == f'read-{phase-1}']
                        assert len(messages) == 1
                        result = json.loads(messages[0]['content'])
                        assert valid_window(result, reads[-1], fixture[:128]), result
                        verified.append(phase // 2)
                        message = {'role': 'assistant', 'content': 'Observed CONTINUITY_42 in fresh source. Browser status is not a page inspection.'}
                        finish = 'stop'
                    phase += 1
                    body = {'model': 'deterministic-local', 'choices': [{'message': message, 'finish_reason': finish}]}
                    code = 200
                except Exception as error:
                    errors.append(traceback.format_exc())
                    code, body = 500, {'error': {'message': 'Acceptance oracle rejected request/result'}}
                data = json.dumps(body).encode()
                self.send_response(code)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Server)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        args = [str(binary), '--root', str(work), '--base-url', f'http://127.0.0.1:{server.server_port}/v1',
                '--model', 'fixture', '--no-stream', '--no-rotation', '--read-only', '--max-steps', '6']
        try:
            seed = subprocess.run(args + ['chat'], input='/exit\n', text=True, capture_output=True, env=env, timeout=20)
            assert seed.returncode == 0
            session_path = next((state / 'sessions').glob('*.json'))
            session = json.loads(session_path.read_text())
            session['messages'] = [{'role': 'system', 'content': 'NEVER MODIFY FILES. Preserve constraints and original evidence. Never replay an uncertain action.'}]
            session_path.write_text(json.dumps(session))
            prompts = [f'الجولة {i}: حلل app.js وتحقق من حالة المتصفح للرابط https://example.test/app دون فتحه. ' + 'حافظ على الأدلة والنطاق والقيود. ' * 8 for i in range(30)]
            started = time.monotonic()
            stops = []
            for start in range(0, 30, 10):
                process = subprocess.run(args + ['--resume', session['id'], 'chat'],
                                         input='\n'.join(prompts[start:start+10] + ['/exit'])+'\n',
                                         text=True, capture_output=True, env=env, timeout=120)
                stops.append(process.stderr.count('exceed the context budget'))
            saved = json.loads(session_path.read_text())
            preserved = sum(any(m.get('role') == 'user' and m.get('content') == prompt.strip() for m in saved['messages']) for prompt in prompts)
            if require_success:
                assert preserved == 30, (preserved, process.stderr[-2000:], process.stdout[-2000:])
            assert (work / 'app.js').read_bytes() == fixture
            assert not errors, errors
            browser_calls = sum(o['tool'] == 'browser' for o in saved.get('observations', []))
            if require_success:
                assert len(verified) == 30 and len(packets) == 60, (len(verified), len(packets), saved['last_report'])
                assert browser_calls == 30
                assert saved['last_report']['context']['effective_context_tokens'] == 32768
            return {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'verified_exchanges': len(verified),
                    'provider_requests': len(packets), 'native_browser_status_calls': browser_calls,
                    'context_error_mentions': sum(stops), 'elapsed_seconds': round(time.monotonic()-started, 3),
                    'original_user_messages_preserved': preserved, 'fixture_unchanged': True,
                    'last_context': saved['last_report'].get('context'), 'packets': packets,
                    'limits': 'Scripted loopback provider, native file tools and browser status only; no DOM/Chromium, live reasoning, real quota, or phone capacity claim. RPM=120 explicitly for fixture; three CLI processes resume the same session.'}
        finally:
            server.shutdown()
            server.server_close()


if __name__ == '__main__':
    negative_controls()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps({'before': run(args.before.resolve(), False), 'after': run(args.after.resolve(), True)}, ensure_ascii=False, indent=2))
