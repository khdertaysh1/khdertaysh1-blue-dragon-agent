#!/usr/bin/env python3
"""Actual CLI + deterministic loopback model: child proposals, parent approval and rollback."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def run(binary, mode):
    state = {'phase': 'spawn', 'errors': [], 'requests': 0, 'child_requests': 0, 'results': []}
    originals = {'a.txt': 'original A\n', 'b.txt': 'قديم B 🐉\n'}
    replacements = {'a.txt': 'revised A\n', 'b.txt': 'جديد B 🐉\n'}
    positive = mode in ('success', 'crlf')
    if mode == 'crlf':
        originals = {p: v.replace('\n', '\r\n') for p, v in originals.items()}
        replacements = {p: v.replace('\n', '\r\n') for p, v in replacements.items()}
    def call(name, args, ident):
        return {'role': 'assistant', 'content': 'Deterministic fixture operation', 'tool_calls': [
            {'id': ident, 'type': 'function', 'function': {'name': name, 'arguments': json.dumps(args)}}]}
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            try:
                packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                messages = packet['messages']
                state['requests'] += 1
                results = [json.loads(m['content']) for m in messages if m['role'] == 'tool']
                child = 'independent read-only research delegate' in str(messages[0]['content'])
                if child:
                    state['child_requests'] += 1
                    if state['child_requests'] == 1:
                        assert any(t['function']['name'] == 'agent_propose_edits' for t in packet['tools'])
                        files = [{'path': p, 'expected_sha256': digest(old), 'content': replacements[p]} for p, old in originals.items()]
                        message = call('agent_propose_edits', {'files': files}, 'propose')
                        message['tool_calls'] += call('agent_propose_edits', {'files': [{'path': 'private.txt', 'expected_sha256': digest('private fixture'), 'content': 'stolen'}]}, 'outside')['tool_calls']
                        message['tool_calls'] += call('write_file', {'path': 'a.txt', 'content': 'direct forbidden'}, 'direct')['tool_calls']
                        message['tool_calls'] += call('agent_propose_edits', {'files': [{'path': 'a.txt', 'expected_sha256': digest(originals['a.txt']), 'content': 'DANGEROUS_CHANGE\r+SAFE_TEXT'}]}, 'bare-cr')['tool_calls']
                    else:
                        assert results[3]['ok'] is False and 'Bare carriage return' in results[3]['error']
                        assert results[0]['workspace_writes'] == 0 and results[0]['ok'] is True
                        assert results[1]['denied'] is True and results[2]['denied'] is True
                        assert all((workspace / p).read_bytes().decode() == value for p, value in originals.items())
                        message = {'role': 'assistant', 'content': 'Proposal only; no edits applied or tests claimed.'}
                else:
                    phase = state['phase']
                    if phase == 'spawn':
                        message = call('delegate_agents', {'action': 'spawn', 'task': 'Suggest bounded changes only', 'paths': list(originals)}, 'spawn')
                        state['phase'] = 'wait'
                    elif phase == 'wait':
                        reports = [r for r in results if 'agents' in r]
                        ready = reports and reports[-1]['agents'] and reports[-1]['agents'][0]['state'] == 'completed'
                        if not ready:
                            message = call('delegate_agents', {'action': 'wait', 'timeout_ms': max(1, 1000 - state['requests'])}, f"wait-{state['requests']}")
                        else:
                            child_row = reports[-1]['agents'][0]
                            evidence = next(e for e in child_row['evidence'] if e['tool'] == 'agent_propose_edits' and e.get('ok') is True)
                            state['pin'] = {'agent_id': child_row['agent_id'], 'evidence_id': evidence['evidence_id'], 'expected_result_sha256': evidence['result_sha256']}
                            message = call('delegate_agents', dict(state['pin'], action='review_edits', expected_result_sha256='0'*64), 'bad-pin')
                            state['phase'] = 'review'
                    elif phase == 'review':
                        assert results[-1]['ok'] is False, results[-1]
                        message = call('delegate_agents', dict(state['pin'], action='review_edits'), 'review')
                        state['phase'] = 'apply'
                    elif phase == 'apply':
                        assert results[-1]['workspace_writes'] == 0 and results[-1]['approval_granted'] is False
                        assert 'a/a.txt' in results[-1]['diff'] and 'a/b.txt' in results[-1]['diff']
                        if mode == 'conflict':
                            (workspace / 'b.txt').write_bytes(b'external change')
                        message = call('delegate_agents', dict(state['pin'], action='apply_edits'), 'apply')
                        state['phase'] = 'applied'
                    elif phase == 'applied':
                        state['apply_result'] = results[-1]
                        if not positive:
                            assert results[-1]['ok'] is False
                            if mode == 'denied':
                                assert results[-1]['denied'] is True
                            message = {'role': 'assistant', 'content': 'Denied/conflicting proposal was not applied.'}
                        else:
                            assert results[-1]['state'] == 'committed', results[-1]
                            assert all((workspace / p).read_bytes().decode() == value for p, value in replacements.items())
                            message = call('delegate_agents', dict(state['pin'], action='apply_edits'), 'repeat')
                            state['phase'] = 'rollback'
                    elif phase == 'rollback':
                        assert results[-1]['ok'] is False
                        message = call('delegate_agents', dict(state['pin'], action='rollback_edits'), 'rollback')
                        state['phase'] = 'done'
                    else:
                        assert results[-1]['state'] == 'rolled_back', results[-1]
                        message = {'role': 'assistant', 'content': 'Pinned proposal applied then explicitly rolled back; original files restored.'}
                body = json.dumps({'choices': [{'message': message}]}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except Exception as error:
                state['errors'].append(repr(error))
                self.send_error(500)
    with tempfile.TemporaryDirectory(prefix='blue-delegate-edits-') as temp:
        root = Path(temp)
        workspace = root / 'workspace'
        workspace.mkdir()
        for name, content in originals.items():
            (workspace / name).write_bytes(content.encode())
        (workspace / 'private.txt').write_bytes(b'private fixture')
        app = root / 'state'
        env = {k: v for k, v in os.environ.items() if k.upper() in {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}}
        env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(app), XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'), NO_COLOR='1', TERM='dumb')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            args = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1', '--model', 'fixture', '--no-stream', '--no-rotation', '--max-steps', '18']
            answers = {'success': 'y\ny\ny\n', 'crlf': 'y\ny\ny\n', 'denied': 'y\nn\n', 'conflict': 'y\n'}[mode]
            result = subprocess.run(args + ['chat'], input=('Propose and review changes.\n' + answers + '/exit\n').encode(), text=False, capture_output=True, env=env, cwd=workspace, timeout=40)
            result.stdout = result.stdout.decode()
            result.stderr = result.stderr.decode()
            assert '\r' not in result.stdout.replace('\r\n', '\n'), 'Bare CR reached terminal output'
            if mode == 'crlf': assert '+revised A\\r' in result.stdout, 'CRLF not visible in approval'
            assert result.returncode == 0, result.stderr[-2500:]
            assert not state['errors'], state['errors']
            assert 'apply_result' in state, result.stdout[-3500:]
            assert result.stdout.count('Approve this ONE action?') == {'success': 3, 'crlf': 3, 'denied': 2, 'conflict': 1}[mode]
            assert (workspace/'a.txt').read_bytes().decode() == originals['a.txt']
            assert (workspace/'b.txt').read_bytes().decode() == ('external change' if mode == 'conflict' else originals['b.txt'])
            assert (workspace/'private.txt').read_bytes().decode() == 'private fixture'
            assert not (workspace/'.blue-dragon-edits.lock').exists()
            session_path = next((app/'sessions').glob('*.json'))
            session = json.loads(session_path.read_bytes().decode())
            child_dir = app/'sessions'/'delegates'/session['id']
            journals = list(child_dir.glob('*.journal'))
            assert len(journals) == (1 if positive else 0)
            if journals:
                assert json.loads(journals[0].read_bytes().decode())['state'] == 'rolled_back'
            before = state['requests']
            resumed = subprocess.run(args + ['--resume', session['id'], 'chat'], input='/audit-report\n/report\n/exit\n', text=True, capture_output=True, env=env, cwd=workspace, timeout=15)
            assert resumed.returncode == 0 and state['requests'] == before
            assert (workspace/'a.txt').read_bytes().decode() == originals['a.txt']
            return {'mode': mode, 'child_model_requests': state['child_requests'], 'approved_workspace_writes': positive, 'restart_replayed_actions': 0}
        finally:
            server.shutdown()
            server.server_close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    args = parser.parse_args()
    print(json.dumps({'cases': [run(args.binary.resolve(), mode) for mode in ['success', 'denied', 'conflict', 'crlf']], 'provider': 'deterministic_loopback', 'physical_android_tested': False}, indent=2))
