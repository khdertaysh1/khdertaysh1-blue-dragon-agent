#!/usr/bin/env python3
"""Deterministic Blue Dragon runtime checks, not a model-intelligence benchmark.
Only synthetic data and loopback HTTP. Run against baseline and candidate binaries.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import threading
import time

FIXTURE = 'port=8000\nmode=dev\n'
CASES = ('atomic_edit', 'atomic_rejection', 'denied_write', 'native_exit',
         'cached_symbols', 'interrupted_resume', 'archived_arguments', 'session_identity')


def call(name, arguments, i):
    return {'role': 'assistant', 'content': 'Fixture operation', 'tool_calls': [
        {'id': f'c{i}', 'type': 'function', 'function': {
            'name': name, 'arguments': json.dumps(arguments)}}]}


def run_case(binary, case):
    requests = []
    responses = []
    results = []
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(request)
            message = responses.pop(0) if responses else {'role':'assistant', 'content':'Fixture complete; no semantic correctness claim.'}
            body = json.dumps({'choices':[{'message':message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
    with tempfile.TemporaryDirectory(prefix='blue-runtime-') as temp:
        root = Path(temp)
        workspace = root / 'workspace'
        workspace.mkdir()
        home = root / 'home'
        home.mkdir()
        app = home / 'app'
        target = workspace / 'config.txt'
        target.write_bytes(FIXTURE.encode())
        env = {k: v for k, v in os.environ.items() if k.upper() in
               {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
        env.update(HOME=str(home), USERPROFILE=str(home), BLUE_DRAGON_HOME=str(app),
                   XDG_CONFIG_HOME=str(home / 'config'), XDG_DATA_HOME=str(home / 'data'),
                   XDG_CACHE_HOME=str(home / 'cache'), TERM='dumb', NO_COLOR='1')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        args = [str(binary), '--root', str(workspace), '--base-url',
                f'http://127.0.0.1:{server.server_port}/v1', '--model', 'fixture',
                '--no-stream', '--no-rotation', '--max-steps', '10']
        def invoke(extra=()):
            result = subprocess.run(args + list(extra) + ['run', 'Run only the synthetic fixture; preserve all unrelated files.'],
                                    env=env, cwd=workspace, input='y\n' * 4,
                                    capture_output=True, text=True, timeout=25)
            assert result.returncode == 0, result.stderr[-3000:]
            return result
        def stored():
            paths = list((app / 'sessions').glob('*.json'))
            assert len(paths) == 1
            return paths[0], json.loads(paths[0].read_text())
        started = time.perf_counter()
        try:
            if case in ('interrupted_resume', 'archived_arguments', 'session_identity'):
                invoke()
                path, session = stored()
                if case == 'session_identity':
                    session['id'] = '../escaped-checkpoint'
                    path.write_text(json.dumps(session))
                    requests.clear()
                    attempt = subprocess.run(args + ['--resume', path.stem, 'run', 'Synthetic identity test'],
                                             env=env, cwd=workspace, input='', text=True,
                                             capture_output=True, timeout=25)
                    assert attempt.returncode != 0, 'Mismatched stored session identity was accepted'
                    assert not (app / 'escaped-checkpoint.json').exists(), 'Checkpoint escaped sessions directory'
                    assert not requests, 'Provider contacted before validating session identity'
                    return {'status':'passed', 'elapsed_ms':round((time.perf_counter()-started)*1000,3),
                            'provider_requests':0, 'tool_results':0}
                session['messages'] = [session['messages'][0], {'role':'user','content':'Never repeat the recorded command.'}]
                if case == 'interrupted_resume':
                    session['messages'].append(call('run_command', {'program': sys.executable, 'args':['-c', 'open("replayed", "w").write("bad")']}, 99))
                else:
                    original = call('write_file', {'path':'archive.txt','content':'ARCHIVED_ARGUMENT_SENTINEL ' + 'x' * 160000}, 99)
                    session['messages'].extend([original, {'role':'tool','tool_call_id':'c99','content':'{"ok":true}'}])
                    responses.append(call('recall_context', {'message_index':2, 'count':6000}, 1))
                path.write_text(json.dumps(session))
                requests.clear()
                invoke(['--resume', session['id']])
            else:
                operations = []
                if case.startswith('atomic_'):
                    edits = [{'find':'8000','replace':'9000'}, {'find':'dev' if case == 'atomic_edit' else 'ABSENT','replace':'test'}]
                    operations = [('multi_edit_file', {'path':'config.txt', 'expected_sha256':hashlib.sha256(FIXTURE.encode()).hexdigest(), 'edits':edits})]
                elif case == 'denied_write':
                    operations = [('write_file', {'path':'config.txt', 'content':'BAD'})]
                elif case == 'native_exit':
                    operations = [('run_command', {'program':sys.executable, 'args':['-c','print("all tests passed"); raise SystemExit(7)']})]
                elif case == 'cached_symbols':
                    for i in range(120):
                        (workspace / f'module{i}.py').write_text(f'# def FakeTarget(): pass\ndef Worker{i}():\n    return {i}\n')
                    operations = [('code_search', {'query':'Worker42', 'kind':'symbols'})] * 2
                responses.extend(call(name, values, i) for i, (name, values) in enumerate(operations))
                invoke(['--read-only'] if case == 'denied_write' else [])
            _, session = stored()
            results = [json.loads(m['content']) for m in session['messages'] if m.get('role') == 'tool']
            if case == 'atomic_edit':
                assert target.read_text() == 'port=9000\nmode=test\n', results
                assert results[0]['ok'] is True
            elif case == 'atomic_rejection':
                assert target.read_text() == FIXTURE and results[0]['ok'] is False, results
            elif case == 'denied_write':
                assert target.read_text() == FIXTURE and results[0]['ok'] is False and results[0].get('status') == 'denied by user', results
            elif case == 'native_exit':
                assert results[0]['exit_code'] == 7, results
            elif case == 'cached_symbols':
                assert results[0]['matches'][0]['path'] == 'module42.py', results
                assert results[1]['coverage']['parses_reused'] >= 120, results
            elif case == 'interrupted_resume':
                assert not (workspace / 'replayed').exists()
                assert results[0]['execution_status'] == 'unknown', results
                assert len(requests) == 1
            elif case == 'archived_arguments':
                assert 'ARCHIVED_ARGUMENT_SENTINEL' in results[-1]['excerpt'], results[-1]
                assert not (workspace / 'archive.txt').exists()
                assert session['last_report']['context']['projection_level'] > 0
            return {'status':'passed', 'elapsed_ms':round((time.perf_counter()-started)*1000, 3),
                    'provider_requests':len(requests), 'tool_results':len(results)}
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=3)


def measure(binary, repeats):
    binary = Path(binary).resolve()
    rows = []
    for case in CASES:
        samples = []
        for _ in range(repeats):
            try:
                samples.append(run_case(binary, case))
            except Exception as exc:
                samples.append({'status':'failed', 'reason':str(exc)[:2000]})
        passed = [r for r in samples if r['status'] == 'passed']
        rows.append({'case':case, 'passed':len(passed), 'executed':repeats,
                     'median_ms':statistics.median(r['elapsed_ms'] for r in passed) if passed else None,
                     'samples':samples})
    return {'binary':str(binary), 'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
            'binary_bytes':binary.stat().st_size, 'cases':rows}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--baseline')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--output', required=True)
    opts = parser.parse_args()
    if not 1 <= opts.repeats <= 10:
        parser.error('repeats must be 1..10')
    report = {'schema':1, 'scope':'Blue Dragon deterministic runtime; not model quality or Alpha parity',
              'platform':platform.platform(), 'fixture_sha256':hashlib.sha256(FIXTURE.encode()).hexdigest(),
              'harness_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'candidate':measure(opts.candidate, opts.repeats),
              'alpha':{'status':'not_run', 'reason':'No validated Alpha adapter/executable supplied'}}
    if opts.baseline:
        report['baseline'] = measure(opts.baseline, opts.repeats)
    Path(opts.output).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    sys.exit(0 if all(r['passed'] == r['executed'] for r in report['candidate']['cases']) else 1)
