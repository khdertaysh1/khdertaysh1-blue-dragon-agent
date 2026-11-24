"""Exercise the real one-shot CLI against a local fixture provider."""
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


def main(binary):
    packets = []

    class Fixture(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            message = {'role': 'assistant', 'content': 'fixture answer'}
            body = json.dumps({'choices': [{'message': message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    with tempfile.TemporaryDirectory() as directory, http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        workspace = Path(directory) / 'work'
        workspace.mkdir()
        (workspace / 'AGENTS.md').write_text('Project convention fixture 2419', encoding='utf-8')
        env = dict(os.environ, BLUE_DRAGON_HOME=str(Path(directory) / 'state'))
        cli = [str(binary), '--root', str(workspace), '--base-url', f'http://127.0.0.1:{server.server_port}/v1',
               '--model', 'fixture', '--no-stream', '--read-only']

        def run(*args, stdin=''):
            return subprocess.run([*cli, *args], input=stdin, text=True, capture_output=True,
                                  env=env, check=True, timeout=20).stdout

        single = json.loads(run('run', '--json', 'Answer briefly'))
        assert single['ok'] and single['answer'] == 'fixture answer', single
        assert any('2419' in m['content'] and m['role'] == 'user' for m in packets[-1]['messages'])
        run('--resume', single['session'], 'chat', stdin='/compact\n/exit\n')
        rows = [json.loads(line) for line in run('--resume', single['session'], 'run', '--ndjson', 'Say hello').splitlines()]
        assert rows[-1]['event'] == 'result' and rows[-1]['ok'], rows
        assert any(r.get('event') == 'answer' for r in rows)
        session = json.loads((Path(env['BLUE_DRAGON_HOME']) / 'sessions' / (single['session'] + '.json')).read_text())
        assert session['compact_context'] and session['last_report']['context']['projection_level'] >= 1
        server.shutdown()
        print('JSON, NDJSON, project conventions and resumed compact projection passed')


if __name__ == '__main__':
    main(Path(sys.argv[1]).resolve())
