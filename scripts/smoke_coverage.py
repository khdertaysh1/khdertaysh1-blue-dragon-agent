#!/usr/bin/env python3
"""Rebuild coverage from a synthetic checkpoint via the real CLI; no network collection."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
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
            message = call('audit_coverage', {}, 'inspect') if len(packets) == 1 else {
                'role': 'assistant', 'content': 'Coverage rechecked; no security test completion inferred.'}
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
        with tempfile.TemporaryDirectory(prefix='blue-coverage-') as temp:
            root = Path(temp)
            workspace = root / 'workspace'
            workspace.mkdir()
            app = root / 'state'
            sessions = app / 'sessions'
            sessions.mkdir(parents=True)
            session = {'id': 'coverage-fixture', 'workspace': str(workspace.resolve()),
                       'model': 'fixture', 'updated_at': '2026-01-01T00:00:00Z',
                       'messages': [{'role':'system','content':'Synthetic coverage fixture'}],
                       'observations': [],
                       'scope': {'enabled': True, 'grants': [{'origin':'https://example.test', 'prefix':'/allowed', 'level':'read', 'account_label':'fixture'}]}}
            for index, (url, tool, result) in enumerate([
                ('https://example.test/allowed?token=PRIVATE_INPUT', 'web_fetch', {'ok':True, 'status':200, 'text':'partial', 'truncated':True}),
                ('https://example.test/outside', 'browser', {'ok':False, 'execution_status':'unknown'}),
                ('https://example.test/altered', 'web_fetch', {'ok':True, 'status':200, 'text':'altered'})]):
                ident = 'c' + str(index)
                session['messages'].append(call(tool, {'url':url}, ident))
                position = len(session['messages'])
                session['observations'].append({'id':f'E{position:06}', 'message_index':position, 'tool':tool, 'ok':True, 'excerpt':'Fixture',
                                                **({'result_sha256':'0'*64} if index==2 else {})})
                session['messages'].append({'role':'tool','tool_call_id':ident,'content':json.dumps(result)})
            env = {k: v for k, v in os.environ.items() if k.upper() in
                   {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}}
            env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(app),
                       XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'),
                       TERM='dumb', NO_COLOR='1')
            command = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1',
                       '--model', 'fixture', '--no-stream', '--no-rotation', '--resume', 'coverage-fixture',
                       'run', 'Review saved coverage only; do not fetch or replay anything.']
            # Use the CLI's persisted identity: Rust canonical paths on Windows
            # include a verbatim prefix that Python Path.resolve() does not keep.
            resume_index = command.index('--resume')
            seeded = subprocess.run(command[:resume_index] + ['chat'], env=env, cwd=workspace,
                                    input='/exit\n', text=True, capture_output=True, timeout=30)
            assert seeded.returncode == 0, seeded.stderr[-3000:]
            paths = list(sessions.glob('*.json'))
            assert len(paths) == 1, paths
            path = paths[0]
            identity = json.loads(path.read_text())
            session.update(id=identity['id'], workspace=identity['workspace'])
            path.write_text(json.dumps(session))
            command[resume_index + 1] = session['id']
            assert not packets, 'Creating the checkpoint contacted the model'
            completed = subprocess.run(command, env=env, cwd=workspace, input='', text=True,
                                       capture_output=True, timeout=30)
            assert completed.returncode == 0, completed.stderr[-3000:]
            saved = json.loads(path.read_text())
            results = [json.loads(m['content']) for m in saved['messages'] if m['role'] == 'tool']
            report = next(r for r in results if 'coverage' in r)
            assert len(report['coverage']) == 2, report
            allowed = next(r for r in report['coverage'] if '/allowed' in r['target'])
            unknown = next(r for r in report['coverage'] if '/outside' in r['target'])
            assert allowed['partial_observations'] == 1 and allowed['currently_in_read_scope'] is True
            assert unknown['unknown_observations'] == 1 and unknown['successful_observations'] == 0
            assert unknown['currently_in_read_scope'] is False
            assert all(r['security_test_completed'] is False and r['automatic_retry_allowed'] is False for r in report['coverage'])
            assert report['provenance']['unavailable_or_mismatched_evidence'] == 1
            assert report['provenance']['stored_ledger_counters_trusted'] is False
            assert report['network_requests'] == 0 and report['fresh_evidence'] is False
            assert 'PRIVATE_INPUT' not in json.dumps(report)
            assert 'APPROVAL REQUIRED' not in completed.stdout
            assert len(packets) == 2, len(packets)
            print(json.dumps({'status':'passed','coverage_rows':2,'partial_observations':1,'unknown_observations':1,'rejected_evidence':1,'collection_requests':0,'model_requests':2,
                              'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    main(parser.parse_args().binary.resolve())
