#!/usr/bin/env python3
"""CLI failure/continuation contract without launching a browser or live provider."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def main(binary, regression):
    packets = []
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            body = json.dumps({'choices': [{'message': {'role': 'assistant', 'content': 'Inspection failure is historical evidence, not a successful browser test.'}, 'finish_reason': 'stop'}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    rows = []
    try:
        with tempfile.TemporaryDirectory(prefix='blue-inspect-exit-') as folder:
            root = Path(folder)
            base = f'http://127.0.0.1:{server.server_port}'
            cases = [
                ('read_only_inspect', ['--read-only', 'inspect', base], '', {}, False),
                ('read_only_run', ['--read-only', 'run', base], '', {}, False),
                ('read_only_url_alias', ['--read-only', base], '', {}, False),
                ('invalid_scheme', ['inspect', 'file:///unrelated'], '', {}, False),
                ('missing_browser', ['inspect', base], '', {}, False),
                ('declined_setup', ['inspect', base], 'n\n', {'TERMUX_VERSION': 'synthetic-fixture'}, False),
                ('interactive_read_only', ['--read-only', 'chat'], base+'\nDiscuss the recorded failure.\n/exit\n', {}, True),
                ('interactive_missing_browser', ['chat'], base+'\nDiscuss the recorded failure.\n/exit\n', {}, True),
            ]
            for name, tail, stdin, extra, interactive in cases:
                home = root/name
                env = {k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')}
                env.update(HOME=str(home), USERPROFILE=str(home), BLUE_DRAGON_HOME=str(home), BLUE_DRAGON_BROWSER_BIN=str(root/'intentionally-absent-browser'), NO_COLOR='1', TERM='dumb')
                env.update(extra)
                count = len(packets)
                result = subprocess.run([str(binary),'--root',str(root),'--base-url',base+'/v1','--model','fixture','--no-stream','--no-rotation']+tail,input=stdin,text=True,capture_output=True,env=env,timeout=30)
                sent = len(packets)-count
                if regression:
                    assert result.returncode == 0 and sent == 1, (result.returncode,sent,result.stderr)
                    print(json.dumps({'status':'regression_reproduced','case':name,'requests':sent,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
                    return
                if interactive:
                    assert result.returncode == 0 and sent == 2, (name,result.returncode,sent,result.stderr)
                    assert 'unknown' in json.dumps(packets[count]['messages'])
                else:
                    assert result.returncode != 0 and sent == 0, (name,result.returncode,sent,result.stderr)
                sessions = list((home/'sessions').glob('*.json'))
                if name != 'invalid_scheme':
                    assert len(sessions)==1, name
                    saved=json.loads(sessions[0].read_text())
                    failures=[o for o in saved['observations'] if o['tool']=='browser_url_inspection']
                    assert len(failures)==1 and failures[0]['ok'] is False, (name,failures)
                    # Automatic browser evidence is also in the audit/working wrapper;
                    # do not require a model-authored status to establish failure.
                    assert 'unknown' in json.dumps(saved['messages'])
                    if not interactive:
                        assert saved['last_report']['finished'] is False
                        assert any(a['tool']=='browser_url_inspection' and a['ok'] is False for a in saved['last_report']['actions'])
                assert not (home/'browser-install.log').exists(), name
                rows.append({'case':name,'exit_code':result.returncode,'provider_requests':sent,'status':'passed'})
            print(json.dumps({'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'cases':rows,'limits':'Scripted loopback provider; no browser launch, installation, live provider or phone test.'}))
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('--expect-regression',action='store_true')
    args=parser.parse_args()
    main(args.binary.resolve(),args.expect_regression)
