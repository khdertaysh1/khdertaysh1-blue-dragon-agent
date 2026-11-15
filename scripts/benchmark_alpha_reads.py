#!/usr/bin/env python3
"""Small common read contracts over real CLIs and a deterministic loopback model.
This measures tool plumbing, not model quality, safety parity or overall capability.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import threading
import time

CASES = ('small', 'utf8', 'range', 'missing')
FIXTURES = {'small': 'BEGIN_SENTINEL\nVALUE_SENTINEL\nEND_SENTINEL\n',
            'utf8': 'أهلاً بالعالم 🐉\n', 'range': 'BEFORE_SENTINEL\nSELECTED_SENTINEL\nAFTER_SENTINEL\n'}


def validate_result(text, engine, case, content):
    """Judge status before content; missing is a specific error, not any failure."""
    try: native = json.loads(text)
    except (ValueError, TypeError): native = None
    if isinstance(native, dict):
        blocked = (native.get('denied') is True or native.get('execution_status') == 'unknown'
                   or any(native.get(k) is True for k in ('cached', 'cached_result', 'reused_local_copy', 'stale'))
                   or native.get('fresh_evidence') is False
                   or 'reused_evidence_id' in native or 'original_evidence_id' in native)
        status = str(native.get('status', '')).lower()
        blocked = blocked or any(word in status for word in ('denied', 'blocked', 'unknown', 'stale', 'cached'))
        if blocked: raise AssertionError('Denied, unknown or stale native result is not fresh read evidence')
        if case != 'missing' and (native.get('ok') is False or native.get('error') or status in ('error', 'failed', 'failure')):
            raise AssertionError('Failed native result cannot establish successful content')
    if case != 'missing' and not isinstance(native, dict):
        assert not text.lstrip().lower().startswith(('error:', 'error executing', 'permission denied')), 'Native error text is not read evidence'
    if case == 'missing':
        assert not (isinstance(native, dict) and native.get('ok') is True), 'Successful result cannot establish missing-file error'
        if engine == 'blue':
            assert isinstance(native, dict) and native.get('ok') is False, 'Missing requires native failure'
            error = str(native.get('error', ''))
        else:
            error = str(native.get('error', '')) if isinstance(native, dict) else text
        lower = error.lower()
        assert not any(word in lower for word in ('permission', 'denied', 'unknown', 'cached', 'stale')), 'Unrelated failure is not missing-file evidence'
        assert 'not found' in lower or 'no such file' in lower, 'Missing-file error was not established'
        return
    visible = native.get('content', text) if isinstance(native, dict) else text
    assert isinstance(visible, str), 'Read result content must be text'
    if case == 'range':
        assert 'SELECTED_SENTINEL' in visible and 'BEFORE_SENTINEL' not in visible and 'AFTER_SENTINEL' not in visible, text
    else:
        cursor = 0
        for line in content.splitlines():
            found = visible.find(line, cursor)
            assert found >= 0, text
            cursor = found + len(line)


def run_case(binary, engine, case):
    tool = 'read' if engine == 'alpha' else 'read_file_window'
    content = FIXTURES.get(case, '')
    filename = 'missing.txt' if case == 'missing' else 'fixture.txt'
    if engine == 'alpha':
        arguments = {'file_path': filename, 'offset': 1 if case == 'range' else 0,
                     'limit': 1 if case == 'range' else 10}
    else:
        arguments = {'path': filename, 'byte_offset': len('BEFORE_SENTINEL\n') if case == 'range' else 0,
                     'byte_count': len('SELECTED_SENTINEL\n') if case == 'range' else 512}
    requests, results, errors = [], [], []
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_POST(self):
            try:
                size = int(self.headers.get('Content-Length', '0'))
                if not 0 < size <= 4 * 1024 * 1024: raise ValueError('request body outside fixture budget')
                packet = json.loads(self.rfile.read(size)); requests.append(packet)
                if len(requests) > 4: raise ValueError('fixture request budget exhausted')
                tools = {t['function']['name'] for t in packet.get('tools', []) if t.get('type') == 'function'}
                if len(requests) == 1:
                    if tool not in tools: raise ValueError('adapter tool was not advertised: ' + tool)
                    message = {'role': 'assistant', 'content': None, 'tool_calls': [
                        {'id': 'fixture_read', 'type': 'function', 'function': {'name': tool, 'arguments': json.dumps(arguments)}}]}
                else:
                    results.extend(m['content'] for m in packet.get('messages', [])
                                   if m.get('role') == 'tool' and m.get('tool_call_id') == 'fixture_read')
                    message = {'role': 'assistant', 'content': 'Fixture finished. No semantic or parity claim.'}
                if packet.get('stream'):
                    delta = dict(message); delta.pop('role', None)
                    for i, call in enumerate(delta.get('tool_calls', [])): call['index'] = i
                    chunk = {'id': 'fixture', 'object': 'chat.completion.chunk', 'model': 'fixture',
                             'choices': [{'index': 0, 'delta': delta, 'finish_reason': None}]}
                    finish = {'id': 'fixture', 'object': 'chat.completion.chunk', 'choices': [
                        {'index': 0, 'delta': {}, 'finish_reason': 'tool_calls' if 'tool_calls' in message else 'stop'}]}
                    body = ('data: ' + json.dumps(chunk) + '\n\ndata: ' + json.dumps(finish) + '\n\ndata: [DONE]\n\n').encode()
                    mime = 'text/event-stream'
                else:
                    body = json.dumps({'id': 'fixture', 'object': 'chat.completion', 'model': 'fixture',
                        'choices': [{'index': 0, 'message': message, 'finish_reason': 'tool_calls' if 'tool_calls' in message else 'stop'}]}).encode()
                    mime = 'application/json'
                self.send_response(200); self.send_header('Content-Type', mime)
                self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
            except Exception as exc:
                errors.append(str(exc)); self.send_error(500)
    with tempfile.TemporaryDirectory(prefix='blue-alpha-read-') as temp:
        root = Path(temp); workspace = root/'workspace'; workspace.mkdir(); home = root/'home'; home.mkdir()
        if case != 'missing': (workspace/filename).write_bytes(content.encode())
        env = {k: v for k, v in os.environ.items() if k.upper() in {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}}
        env.update(HOME=str(home), USERPROFILE=str(home), ALPHACODE_HOME=str(home/'alpha'),
                   BLUE_DRAGON_HOME=str(home/'blue'), XDG_CONFIG_HOME=str(home/'config'),
                   XDG_DATA_HOME=str(home/'data'), XDG_CACHE_HOME=str(home/'cache'), TERM='dumb', NO_COLOR='1')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        endpoint = f'http://127.0.0.1:{server.server_port}/v1'
        prompt = 'Run only the requested synthetic read fixture. Do not modify any files or call other tools.'
        if engine == 'alpha':
            env.update(ALPHACODE_OPENAI_COMPAT_API_BASE=endpoint, OPENAI_COMPAT_API_KEY='synthetic-fixture-key',
                       ALPHACODE_RUN_MCP='0', ALPHACODE_RUN_AUTO_POKE='0')
            command = [str(binary), '--provider', 'openai-compatible', '--model', 'fixture',
                       '--no-update', '--no-selfdev', '-C', str(workspace), 'run', '--json', prompt]
        else:
            command = [str(binary), '--root', str(workspace), '--base-url', endpoint, '--model', 'fixture',
                       '--no-stream', '--no-rotation', '--max-steps', '4', 'run', prompt]
        started = time.perf_counter()
        try:
            run = subprocess.run(command, input='', text=True, capture_output=True, env=env, cwd=workspace, timeout=45)
            if run.returncode: raise AssertionError(f'CLI exit {run.returncode}: {run.stderr[-1500:]}')
            if errors: raise AssertionError(errors)
            if not results: raise AssertionError('No paired native tool result reached the fixture provider')
            result = results[-1]
            text = result if isinstance(result, str) else json.dumps(result, ensure_ascii=False)
            validate_result(text, engine, case, content)
            if case != 'missing': assert (workspace/filename).read_bytes() == content.encode(), 'fixture modified'
            return {'status': 'passed', 'elapsed_ms': round((time.perf_counter()-started)*1000, 3),
                    'provider_requests': len(requests), 'tool': tool, 'arguments': arguments,
                    'tool_result_sha256': hashlib.sha256(text.encode()).hexdigest()}
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=3)


def measure(binary, engine, repeats):
    binary = Path(binary).resolve()
    record = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'binary_bytes': binary.stat().st_size, 'cases': []}
    for case in CASES:
        for repetition in range(repeats):
            try: sample = run_case(binary, engine, case)
            except Exception as exc: sample = {'status': 'failed_or_adapter_blocked', 'reason': str(exc)[:2000]}
            record['cases'].append({'case': case, 'repetition': repetition + 1, **sample})
    return record


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--blue', required=True, type=Path); p.add_argument('--alpha', type=Path)
    p.add_argument('--output', required=True, type=Path); p.add_argument('--repeats', type=int, default=1)
    args = p.parse_args()
    if not 1 <= args.repeats <= 3: p.error('repeats must be 1..3')
    # Refuse overwrite before executing any fixture.
    with args.output.open('x') as output:
        report = {'schema': 1, 'scope': 'Four deterministic read contracts, not model quality or overall parity',
                  'platform': platform.platform(), 'harness_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  'fixtures_sha256': hashlib.sha256(json.dumps(FIXTURES, sort_keys=True).encode()).hexdigest(),
                  'limits': ['Byte versus line coordinates are adapted explicitly.',
                             'Startup-inclusive elapsed times include different builds/configurations and are not a speed ranking.',
                             'Synthetic model and fresh homes only; no live providers, browser or device checks.',
                             'Failure may reflect the adapter; inspect native evidence before attributing product defects.'],
                  'blue': measure(args.blue, 'blue', args.repeats),
                  'alpha': measure(args.alpha, 'alpha', args.repeats) if args.alpha else {'status': 'not_run', 'reason': 'No executable provided'}}
        json.dump(report, output, indent=2); output.write('\n')
    print(json.dumps({engine: sum(r['status']=='passed' for r in report[engine].get('cases', [])) for engine in ['blue', 'alpha']}))
    raise SystemExit(0 if all(r['status']=='passed' for e in ['blue','alpha'] for r in report[e].get('cases', [])) else 1)
