#!/usr/bin/env python3
"""Real CLI/loopback regression for high-RPM state crossing the old 64KiB bound."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time


def run(binary):
    requests = []
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass
        def do_POST(self):
            requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            data = json.dumps({'choices': [{'finish_reason': 'stop', 'message': {'role': 'assistant', 'content': 'Done.'}}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
    with tempfile.TemporaryDirectory(prefix='blue-budget-state-') as directory:
        root = Path(directory)
        (root/'project').mkdir()
        env = {k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')}
        env.update(HOME=str(root), USERPROFILE=str(root), BLUE_DRAGON_HOME=str(root/'state'),
                   BLUE_DRAGON_RPM='10000', TERM='dumb', NO_COLOR='1')
        server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        command = [str(binary.resolve()), '--root', str(root/'project'), '--base-url',
                   f'http://127.0.0.1:{server.server_port}/v1', '--model', 'fixture',
                   '--read-only', '--no-stream', '--no-rotation', '--max-steps', '2', 'run', 'Say Done. No tools needed.']
        def dispatch():
            before = len(requests)
            result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, result.stderr
            assert len(requests) == before + 1, 'Expected exactly one provider dispatch'
        try:
            dispatch()
            path = next((root/'state/provider-budgets').glob('*.json'))
            now = int(time.time())
            state = {'schema':1, 'window':now, 'requests':0, 'until':0, 'models':{},
                     'events':[{'at':now, 'tokens':1000} for _ in range(2045)]}
            path.write_text(json.dumps(state,separators=(',',':')))
            initial = path.stat().st_size
            assert initial == 65518
            dispatch()
            crossed = path.stat().st_size
            assert crossed == 65551, crossed
            dispatch()
            reread = path.stat().st_size
            assert reread > crossed
            state = json.loads(path.read_text())
            state['window'] = int(time.time()) - 61
            for event in state['events']:
                event['at'] = state['window']
            path.write_text(json.dumps(state,separators=(',',':')))
            dispatch()
            state = json.loads(path.read_text())
            assert len(state['events']) == 1 and state['requests'] == 1
            return {'initial_events':2045, 'initial_bytes':initial, 'after_dispatch_bytes':crossed,
                    'after_next_dispatch_bytes':reread, 'after_expiry_events':len(state['events']),
                    'provider_dispatches':len(requests), 'network':'loopback only', 'passed':True}
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=5)

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary', type=Path)
    args = p.parse_args()
    print(json.dumps(run(args.binary), indent=2))
