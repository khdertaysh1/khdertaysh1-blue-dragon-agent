"""Linux PTY regression: exit with a job active and model HTTP request blocked.
No external service or credentials. Pass BLUE_DRAGON_BINARY for a debug build.
"""
import fcntl
import http.server
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import tempfile
import termios
import threading
import time
from pty_helpers import contains_styled_text

started = threading.Event()
release = threading.Event()
class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        results = [m for m in request['messages'] if m.get('role') == 'tool']
        if not results:
            message = {'role':'assistant','content':None,'tool_calls':[{'id':'job1','type':'function','function':{'name':'start_job','arguments':json.dumps({'program':'sh','args':['-c','sleep 2; printf survived > job-survived.txt'],'timeout_seconds':30})}}]}
        else:
            result = json.loads(results[-1]['content'])
            assert result.get('state') == 'running', result
            started.set()
            release.wait(10)
            message = {'role':'assistant','content':'Job still pending.'}
        body=json.dumps({'choices':[{'message':message}]}).encode()
        try:
            self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
threading.Thread(target=server.serve_forever,daemon=True).start()
master,slave=pty.openpty()
process=None
try:
    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',30,100,0,0))
    with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as home:
        env=dict(os.environ,TERM='xterm-256color',BLUE_DRAGON_HOME=home)
        binary=str(Path(os.environ.get('BLUE_DRAGON_BINARY','target/release/blue-dragon')).resolve())
        process=subprocess.Popen([binary,'--root',root,'--model','fixture','--base-url',f'http://127.0.0.1:{server.server_port}/v1','--no-stream','--no-rotation','tui'],stdin=slave,stdout=slave,stderr=slave,env=env)
        output=bytearray()
        def wait_for(text):
            deadline=time.monotonic()+8
            while time.monotonic()<deadline:
                if contains_styled_text(output, text): return
                if select.select([master],[],[],.05)[0]: output.extend(os.read(master,65536))
            raise AssertionError(f'Missing {text!r}: {output[-3000:]!r}')
        wait_for(b'BLUE DRAGON')
        os.write(master,b'Run the approved background fixture\r')
        wait_for(b'APPROVAL REQUIRED')
        os.write(master,b'y')
        wait_for(b'Start background workspace job')
        os.write(master,b'y')
        assert started.wait(5),'Job did not start'
        before=time.monotonic()
        os.write(master,b'\x1b')
        while process.poll() is None and time.monotonic()-before<3:
            if select.select([master],[],[],.05)[0]: os.read(master,65536)
        assert process.poll() == 0,'TUI failed to exit promptly'
        time.sleep(2.2)
        assert not (Path(root)/'job-survived.txt').exists(),'Job survived graceful TUI shutdown'
        print('Passed: TUI exit waits for job cleanup even while provider request is blocked')
finally:
    release.set()
    if process and process.poll() is None: process.kill(); process.wait()
    os.close(master);os.close(slave);server.shutdown();server.server_close()
