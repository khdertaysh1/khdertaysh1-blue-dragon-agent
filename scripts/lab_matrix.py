#!/usr/bin/env python3
"""Opt-in pinned Between Bytes loopback matrix. No browser exploit claim."""
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
from urllib.error import HTTPError
from urllib.parse import urlencode, urlsplit
from urllib.request import build_opener, HTTPRedirectHandler, ProxyHandler
from smoke_continuity import call

LAB_COMMIT = 'c396c90f2e7ed080f71368ce9edb5cd630098b09'


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def loopback_only(event, args):
    if event in {'socket.connect', 'socket.bind'}:
        address = args[1]
        if not isinstance(address, tuple) or address[0] not in ('127.0.0.1', '::1'):
            raise RuntimeError('Non-loopback socket operation denied by matrix harness')


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_):
        return None


def blue_marker_case(binary, base, mode, invalid=False, policy=False):
    packets = []
    endpoint = '/preview/reflected-xss/' + mode
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            packets.append(packet)
            step = len(packets)
            if policy and step == 2:
                prior = [json.loads(m['content']) for m in packet['messages'] if m['role'] == 'tool']
                message = call('review_web_evidence', {'engine': 'response_policy_review', 'evidence_id': prior[-1]['evidence_id']}, 'policy')
            elif policy and step > 2:
                message = {'role': 'assistant', 'content': 'Header indicators only; browser enforcement unverified.'}
            elif step in (1, 3, 5):
                marker = 'VARIANT_MARKER' if step == 3 else 'BASE_MARKER'
                args = {'url': base + endpoint + '?' + urlencode({'q': marker, 'token': 'invalid' if invalid else '0' * 32})}
                message = call('web_fetch', args, 'fetch-' + str(step))
            elif step == 6:
                results = [json.loads(m['content']) for m in packet['messages'] if m['role'] == 'tool']
                assert len(results) == 3
                message = call('review_web_evidence', {'engine': 'reflection_controlled_comparison',
                    'evidence_id': results[0]['evidence_id'], 'variant_evidence': results[1]['evidence_id'],
                    'repeated_control_evidence': results[2]['evidence_id'], 'marker': 'BASE_MARKER', 'variant_marker': 'VARIANT_MARKER'}, 'review')
            else:
                message = {'role': 'assistant', 'content': 'Literal marker evidence only; browser execution is unverified.'}
            payload = json.dumps({'choices': [{'message': message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-lab-matrix-') as temp:
            root = Path(temp)
            workspace = root / 'workspace'
            workspace.mkdir()
            app = root / 'state'
            env = {k: v for k, v in os.environ.items() if k.upper() in {'PATH', 'SYSTEMROOT', 'WINDIR', 'TMP', 'TEMP', 'COMSPEC', 'PATHEXT'}}
            env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(app),
                       XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'), TERM='dumb', NO_COLOR='1')
            cmd = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1',
                   '--model', 'fixture', '--no-stream', '--no-rotation', '--max-steps', '8', 'chat']
            child = subprocess.run(cmd, env=env, cwd=workspace, text=True, capture_output=True, timeout=30,
                input=(f'/scope allow {base}{endpoint} read\nInspect response policy.\ny\n/exit\n' if policy else f'/scope allow {base}{endpoint} read\nNormal control.\ny\nVariant.\ny\nRepeat normal control and compare.\ny\n/exit\n'))
            assert child.returncode == 0, child.stderr[-2000:]
            files = list((app/'sessions').glob('*.json'))
            assert len(files) == 1
            saved = json.loads(files[0].read_text())
            results = [json.loads(m['content']) for m in saved['messages'] if m['role'] == 'tool']
            if policy:
                assert len(results) == 2 and len(packets) == 3
                observed, review = results
                # Separate HTTP oracle inspects the wire header names/values directly.
                url = base + endpoint + '?' + urlencode({'q': 'BASE_MARKER', 'token': 'invalid' if invalid else '0' * 32})
                opener = build_opener(ProxyHandler({}), NoRedirect())
                try:
                    response = opener.open(url, timeout=4)
                except HTTPError as error:
                    response = error
                with response:
                    assert response.status == observed['status']
                    headers = response.headers
                    oracle = {
                        'x_frame_options_known_value_seen': headers.get('X-Frame-Options', '').lower().strip() in ('deny', 'sameorigin'),
                        'csp_header_frame_directive_seen': any(d.strip().lower().startswith('frame-ancestors ') for d in headers.get('Content-Security-Policy', '').split(';')),
                        'report_only_frame_directive_seen': any(d.strip().lower().startswith('frame-ancestors ') for d in headers.get('Content-Security-Policy-Report-Only', '').split(';')),
                        'nosniff_value_seen': headers.get('X-Content-Type-Options', '').lower().strip() == 'nosniff'}
                assert review.get('vulnerability_confirmed') in (None, False)
                if invalid:
                    assert observed['status'] == 400 and review['ok'] is False
                    verdict = 'rejected_unusable_evidence'
                    indicators = None
                else:
                    assert review['ok'] is True and review['network_requests'] == 0
                    assert review['review']['collection_context'] == 'single_native_GET_without_browser_session'
                    row = review['review']['responses'][0]
                    assert row['state'] == 'indicators_only' and row['protection_verified'] is False
                    assert row['evidence_pointer'] == '/security_header_indicators'
                    indicators = {k: row[k] for k in oracle}
                    assert indicators == oracle
                    verdict = 'indicators_match_independent_HTTP_oracle'
                return {'family': 'native_response_policy', 'mode': mode,
                    'condition': 'invalid_preview_token' if invalid else 'valid_preview',
                    'endpoint': endpoint, 'native_http_status': observed['status'],
                    'verdict': verdict, 'indicators': indicators, 'independent_HTTP_oracle': oracle,
                    'vulnerability_confirmed': review.get('vulnerability_confirmed'),
                    'browser_enforcement_verified': False, 'tool_evidence_id': observed['evidence_id'],
                    'response_result_sha256': digest(observed), 'model_calls': len(packets),
                    'expected_collector_GETs': 1, 'independent_oracle_GETs': 1}
            assert len(results) == 4 and len(packets) == 7
            assert len({r['evidence_id'] for r in results[:3]}) == 3
            review = results[-1]
            assert review.get('vulnerability_confirmed') in (None, False)
            if invalid:
                assert review['ok'] is False and all(r['status'] == 400 for r in results[:3])
                verdict = 'rejected_unusable_evidence'
            else:
                assert review['ok'] is True and review['vulnerability_confirmed'] is False
                verdict = review['review']['verdict']
                assert verdict == 'differential_literal_marker_only'
                assert review['review']['execution_observed'] is False
                assert review['review']['causality_proven'] is False
                assert review['network_requests'] == 0
            return {'family': 'xss_marker_review', 'mode': mode, 'condition': 'invalid_preview_token' if invalid else 'valid_preview',
                    'endpoint': endpoint, 'native_http_statuses': [r['status'] for r in results[:3]],
                    'verdict': verdict, 'vulnerability_confirmed': review.get('vulnerability_confirmed'), 'browser_execution_verified': False,
                    'tool_evidence_ids': [r['evidence_id'] for r in results[:3]],
                    'response_result_sha256': [digest(r) for r in results[:3]],
                    'model_calls': len(packets), 'expected_collector_GETs': 3,
                    'limits': 'Harmless markers do not discriminate raw from HTML-escaped rendering; no browser execution performed.'}
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=3)


def main(binary, lab):
    commit = subprocess.check_output(['git', '-C', str(lab), 'rev-parse', 'HEAD'], text=True).strip()
    assert commit == LAB_COMMIT, 'Lab revision has not been reviewed by this matrix'
    subprocess.run(['git', '-C', str(lab), 'diff', '--quiet', 'HEAD', '--'], check=True)
    sys.dont_write_bytecode = True
    sys.addaudithook(loopback_only)
    sys.path.insert(0, str(lab))
    from app import make_server
    server = make_server(0)
    assert server.server_address[0] == server.sidecar.server_address[0] == '127.0.0.1'
    paths = []
    parent = server.RequestHandlerClass
    class Counted(parent):
        def do_GET(self):
            paths.append(('GET', urlsplit(self.path).path))
            assert len(paths) <= 80
            super().do_GET()
        def do_POST(self):
            paths.append(('POST', urlsplit(self.path).path))
            assert len(paths) <= 80
            super().do_POST()
    server.RequestHandlerClass = Counted
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    base = f'http://127.0.0.1:{server.server_port}'
    try:
        rows = []
        for mode, invalid in [('vulnerable', False), ('fixed', False), ('fixed', True)]:
            before = len(paths)
            row = blue_marker_case(binary, base, mode, invalid)
            assert len(paths) - before == 3
            rows.append(row)
        for mode, invalid in [('vulnerable', False), ('fixed', False), ('fixed', True)]:
            before = len(paths)
            rows.append(blue_marker_case(binary, base, mode, invalid, policy=True))
            assert len(paths) - before == 2
        assert len(paths) == 15
        result = {'status': 'passed', 'lab_commit': commit, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                  'matrix_rows': rows, 'coverage': {'selected_rows': len(rows), 'families': ['xss_marker_review', 'native_response_policy'],
                    'lab_http_requests': len(paths), 'blue_dragon_GETs': 12, 'independent_policy_oracle_GETs': 3, 'blue_dragon_rows': 6, 'direct_HTTP_rows': 0, 'external_requests_observed': 0, 'browser_execution_cases': 0,
                    'unsupported_confirmations_on_fixed_or_invalid_marker_cases': sum(r.get('vulnerability_confirmed') is True for r in rows if r['family'] == 'xss_marker_review' and r['mode'] == 'fixed'),
                    'exploit_precision_or_recall': None, 'reason': 'No browser execution oracle; do not treat non-confirmation as detection accuracy.'},
                  'limits': 'Pinned lab, synthetic sessions, loopback HTTP only. Scripted local model is not a live provider.'}
        print(json.dumps(result, ensure_ascii=False))
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=3)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--lab', type=Path, required=True, help='Separate reviewed lab checkout at the pinned commit; this script never clones or modifies it.')
    args = parser.parse_args()
    main(args.binary.resolve(), args.lab.resolve())
