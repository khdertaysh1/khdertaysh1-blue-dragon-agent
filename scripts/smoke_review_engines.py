#!/usr/bin/env python3
"""Real CLI + scripted model + loopback redirect; no live provider or browser."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
from urllib.parse import parse_qs, urlsplit


def run(binary, scoped, comparison=False, reflection=None, redirect_case=None):
    requests = []
    get_paths = []
    evidence_id = None

    class Server(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            get_paths.append(self.path)
            if reflection:
                bodies = {'escaped': ('text/html', '&lt;script&gt;BD_MARKER_123&lt;/script&gt;'),
                          'json': ('application/json', '{"value":"BD_MARKER_123"}'),
                          'static': ('text/plain', 'Static documentation BD_MARKER_123'),
                          'absent': ('text/html', '<p>No marker</p>'),
                          'truncated': ('text/html', 'x' * 65000 + 'BD_MARKER_123')}
                if comparison:
                    marker = parse_qs(urlsplit(self.path).query)['q'][0]
                    mime, body = 'text/html', '&lt;' + marker + '&gt;'
                    if reflection == 'fixed':
                        body = 'BASE_MARKER VARIANT_MARKER'
                    if reflection == 'unstable' and len(get_paths) == 3:
                        body += ' changed control'
                else:
                    mime, body = bodies[reflection]
                payload = body.encode()
                self.send_response(200)
                self.send_header('Content-Type', mime)
                self.send_header('Content-Length', str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            if redirect_case and 'next=variant' in self.path:
                self.send_response(503 if redirect_case == 'site_error' else 403)
                self.send_header('Content-Length', '0')
                self.end_headers()
                return
            if redirect_case == 'unstable' and len(get_paths) == 3:
                self.send_response(302)
                self.send_header('Location', '/changed-control')
                self.send_header('Content-Length', '0')
                self.end_headers()
                return
            self.send_response(302)
            self.send_header('Location', 'https://controlled.invalid/target?token=PRIVATE_DESTINATION' if 'next=variant' in self.path else '/destination?token=PRIVATE_DESTINATION#PRIVATE_FRAGMENT')
            self.send_header('Content-Length', '0')
            self.end_headers()

        def do_POST(self):
            nonlocal evidence_id
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(request)
            if comparison and len(requests) in (1, 3, 5):
                name = 'web_fetch'
                arguments = {'url': base + ('/reflect?q=' + ('VARIANT_MARKER' if len(requests) == 3 else 'BASE_MARKER') if reflection else '/redirect?next=' + ('variant' if len(requests) == 3 else 'normal'))}
            elif comparison and len(requests) == 6:
                prior = [json.loads(m['content']) for m in request['messages'] if m['role'] == 'tool']
                observed = [r for r in prior if isinstance(r.get('status'), int)]
                assert len(observed) == 3, observed
                evidence_id = observed[0]['evidence_id']
                name = 'review_web_evidence'
                arguments = {'engine': 'reflection_controlled_comparison' if reflection else 'redirect_controlled_comparison', 'evidence_id': evidence_id,
                             'variant_evidence': observed[1]['evidence_id'],
                             'repeated_control_evidence': observed[2]['evidence_id']}
                if reflection:
                    arguments.update(marker='BASE_MARKER', variant_marker='VARIANT_MARKER')
            elif not comparison and len(requests) == 1:
                name = 'web_fetch'
                arguments = {'url': base + ('/reflect?q=BD_MARKER_123' if reflection else '/redirect?token=PRIVATE_INPUT')}
            elif not comparison and len(requests) == 2:
                result = json.loads(next(m['content'] for m in reversed(request['messages']) if m['role'] == 'tool'))
                evidence_id = result['evidence_id']
                name = 'review_web_evidence'
                arguments = {'engine': 'reflection_review' if reflection else 'redirect_review', 'evidence_id': evidence_id}
                if reflection:
                    arguments['marker'] = 'BD_MARKER_123'
            else:
                name = None
            message = ({'role': 'assistant', 'content': 'Synthetic review', 'tool_calls': [
                {'id': 'c' + str(len(requests)), 'type': 'function',
                 'function': {'name': name, 'arguments': json.dumps(arguments)}}]}
                if name else {'role': 'assistant', 'content': 'Redirect metadata reviewed; no vulnerability proved.'})
            payload = json.dumps({'choices': [{'message': message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Server)
    base = f'http://127.0.0.1:{server.server_port}'
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-engine-') as temp:
            root = Path(temp)
            home = root / 'home'
            home.mkdir()
            workspace = root / 'workspace'
            workspace.mkdir()
            app = home / 'app'
            env = {k: v for k, v in os.environ.items() if k.upper() in
                   {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}}
            env.update(HOME=str(home), USERPROFILE=str(home), BLUE_DRAGON_HOME=str(app),
                       XDG_CONFIG_HOME=str(home / 'config'), XDG_DATA_HOME=str(home / 'data'),
                       TERM='dumb', NO_COLOR='1')
            args = [str(binary), '--root', str(workspace), '--base-url', base + '/v1',
                    '--model', 'fixture', '--no-stream', '--no-rotation', '--max-steps', '8', 'chat']
            scope = f'/scope allow {base}/ read' if scoped else '/scope clear'
            consent = 'y\n' if scoped else ''
            prompts = 'Normal control.\ny\nVariant.\ny\nRepeat normal control and compare.\ny\n' if comparison else f'Review the synthetic evidence fixture.\n{consent}'
            child = subprocess.run(args, env=env, cwd=workspace, text=True, capture_output=True,
                                   input=f'{scope}\n{prompts}/exit\n', timeout=30)
            assert child.returncode == 0, child.stderr[-3000:]
            files = list((app / 'sessions').glob('*.json'))
            assert len(files) == 1, files
            session = json.loads(files[0].read_text())
            # Validate native checkpoint digests against the exact saved tool pair.
            def digest(value):
                canonical = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
                return hashlib.sha256(canonical.encode()).hexdigest()
            for observation in session['observations']:
                index = observation['message_index']
                message = session['messages'][index]
                assert message['role'] == 'tool'
                result = json.loads(message['content'])
                assert result.pop('evidence_id') == observation['id']
                assert digest(result) == observation['result_sha256']
                assistant = next(m for m in reversed(session['messages'][:index]) if m['role'] == 'assistant')
                call = next(c for c in assistant['tool_calls'] if c['id'] == message['tool_call_id'])
                assert digest(json.loads(call['function']['arguments'])) == observation['request_sha256']
            results = [json.loads(m['content']) for m in session['messages'] if m['role'] == 'tool']
            assert len(results) == (4 if comparison else 2), results
            fetched, review = results[0], results[-1]
            if scoped and reflection and comparison:
                expected = {'differential':'differential_literal_marker_only', 'fixed':'inconclusive_marker_not_differential', 'unstable':'inconclusive_control_changed'}[reflection]
                assert review['ok'] is True and review['engine'] == 'reflection_controlled_comparison', review
                assert review['review']['verdict'] == expected, review
                assert review['review']['execution_observed'] is False
                assert review['review']['causality_proven'] is False
                assert review['vulnerability_confirmed'] is False
                assert review['network_requests'] == 0 and review['state_changes'] == 0
                assert review['fresh_evidence'] is False
                assert 'BASE_MARKER' not in json.dumps(review) and 'VARIANT_MARKER' not in json.dumps(review)
                assert len(get_paths) == 3 and all(p.startswith('/reflect?') for p in get_paths), get_paths
                assert results[0]['evidence_id'] != results[2]['evidence_id']
                assert not results[2].get('cached_result')
            elif scoped and reflection:
                expected = ('inconclusive_incomplete_body' if reflection == 'truncated' else
                            'marker_not_observed_in_captured_text' if reflection == 'absent' else 'literal_marker_observed')
                assert fetched['status'] == 200
                assert review['ok'] is True and review['engine'] == 'reflection_review', review
                assert review['review']['state'] == expected, review
                assert review['review']['execution_observed'] is False
                assert review['review']['input_control_proven'] is False
                assert review['vulnerability_confirmed'] is False
                assert review['network_requests'] == 0 and review['state_changes'] == 0
                assert review['fresh_evidence'] is False
                assert review['source_evidence_id'] == evidence_id
                assert 'BD_MARKER_123' not in json.dumps(review)
                assert len(get_paths) == 1 and get_paths[0].startswith('/reflect?'), get_paths
            elif scoped:
                assert fetched['status'] == 302 and fetched['redirect']['followed'] is False
                assert review['ok'] is True and review['engine'] == ('redirect_controlled_comparison' if comparison else 'redirect_review')
                assert review['evidence_id'] != evidence_id  # Native result envelope has its own observation ID.
                assert review['source_evidence_id'] == evidence_id
                assert review['network_requests'] == 0 and review['state_changes'] == 0
                assert review['vulnerability_confirmed'] is False and review['fresh_evidence'] is False
                if comparison:
                    assert review['review']['control_metadata_consistent'] is (redirect_case != 'unstable')
                    if redirect_case:
                        expected = {'rejected':'variant_http_rejection_observed', 'site_error':'inconclusive_variant_non_redirect_response', 'unstable':'inconclusive_control_changed'}[redirect_case]
                        assert review['review']['verdict'] == expected
                        assert review['review']['destination_metadata_changed'] is None
                        assert review['review']['variant_redirect_observed'] is False
                        assert review['review']['protection_verified'] is False
                        assert results[1]['status'] == (503 if redirect_case == 'site_error' else 403)
                    else:
                        assert review['review']['destination_metadata_changed'] is True
                    assert review['review']['causality_proven'] is False
                    assert results[0]['evidence_id'] != results[2]['evidence_id']
                    assert not results[2].get('cached_result')
                else:
                    assert review['review']['redirect_status_observed'] is True
                assert len(get_paths) == (3 if comparison else 1) and all(p.startswith('/redirect?') for p in get_paths), get_paths
                assert 'PRIVATE_' not in json.dumps(fetched['redirect'])
                assert 'PRIVATE_' not in json.dumps(review)
            else:
                assert fetched['ok'] is False and review['ok'] is False, results
                assert not get_paths, get_paths
            assert len(requests) == (7 if comparison else 3), len(requests)
            print(json.dumps({'status': 'passed', 'scope_case': 'allowed' if scoped else 'denied', 'comparison': comparison, 'reflection_case': reflection, 'redirect_case': redirect_case, 'native_get_requests': len(get_paths),
                              'redirects_followed': 0, 'model_requests': len(requests),
                              'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest()}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    binary = parser.parse_args().binary.resolve()
    for scoped in (True, False):
        run(binary, scoped)

    run(binary, True, comparison=True)

    for reflection in ('escaped', 'json', 'static', 'absent', 'truncated'):
        run(binary, True, reflection=reflection)
    run(binary, False, reflection='escaped')

    for reflection in ('differential', 'fixed', 'unstable'):
        run(binary, True, comparison=True, reflection=reflection)

    for redirect_case in ('rejected', 'site_error', 'unstable'):
        run(binary, True, comparison=True, redirect_case=redirect_case)
