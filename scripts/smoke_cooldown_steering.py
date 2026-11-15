#!/usr/bin/env python3
"""Send scope narrowing AFTER a 429, in CLI and a 40x24 TUI. No live provider."""
import argparse,http.server,json,os,select,subprocess,tempfile,threading,time,hashlib
from pathlib import Path

def trial(binary,tui,baseline):
 with tempfile.TemporaryDirectory(prefix='blue-cooldown-input-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir();state=root/'state';packets=[];hit=threading.Event();capture=bytearray()
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))));first=len(packets)==1
    value={'error':{'code':'tpm_rate_limit_exceeded'}} if first else {'choices':[{'message':{'role':'assistant','content':'No operations executed.'},'finish_reason':'stop'}]}
    raw=json.dumps(value).encode();self.send_response(429 if first else 200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)))
    if first:self.send_header('Retry-After','3')
    self.end_headers();self.wfile.write(raw);hit.set()
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP')};env.update(HOME=tmp,USERPROFILE=tmp,BLUE_DRAGON_HOME=str(state),TERM='xterm-256color' if tui else 'dumb',NO_COLOR='1')
  cmd=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--read-only','tui' if tui else 'chat']
  master=slave=None
  if tui:
   import pty,fcntl,termios,struct
   master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,40,0,0));p=subprocess.Popen(cmd,stdin=slave,stdout=slave,stderr=slave,env=env);read_fd=master
  else:p=subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env);read_fd=p.stdout.fileno()
  def send(text):
   data=(text+('\r' if tui else '\n')).encode()
   if tui:os.write(master,data)
   else:p.stdin.write(data);p.stdin.flush()
  def saved():
   try:return json.loads(next((state/'sessions').glob('*.json')).read_text())
   except (OSError,ValueError,StopIteration):return {}
  def wait_for(check,seconds=7):
   deadline=time.monotonic()+seconds
   while time.monotonic()<deadline:
    if check():return
    if select.select([read_fd],[],[],.03)[0]:
     try:capture.extend(os.read(read_fd,65536))
     except OSError:pass
    assert p.poll() is None,bytes(capture[-800:])
   raise AssertionError(('deadline',tui,len(packets),bytes(capture[-800:])))
  try:
   wait_for(lambda:bool(saved()))
   # Wait for first frame/prompt, then grant a scope that will be withdrawn.
   wait_for(lambda:len(capture)>100)
   send('/scope allow http://127.0.0.1:12345/ read')
   wait_for(lambda:bool(saved().get('scope',{}).get('grants')))
   send('Inspect no network target. Keep original constraint: never execute commands. لا تعد الإجراءات المجهولة.')
   wait_for(hit.is_set)
   # This input is sent in response to the actual HTTP rejection, not preloaded.
   send('/scope clear')
   if tui:
    if baseline:wait_for(lambda:(saved().get('last_report') or {}).get('finished') is True)
    else:wait_for(lambda:(saved().get('last_report') or {}).get('cancelled') is True)
    cancelled=(saved().get('last_report') or {}).get('cancelled',False)
    # Saved report precedes the UI Finished event; drain frames before Enter.
    deadline=time.monotonic()+.3
    while time.monotonic()<deadline:
     if select.select([read_fd],[],[],.03)[0]:capture.extend(os.read(read_fd,65536))
    send('') # submit the preserved draft after cancellation; never retype it
   else:cancelled=False
   if not baseline:wait_for(lambda:saved().get('scope',{}).get('grants')==[])
   # Observe beyond the old retry deadline; no old prompt may be dispatched.
   end=time.monotonic()+4.3
   while time.monotonic()<end:
    if select.select([read_fd],[],[],.05)[0]:capture.extend(os.read(read_fd,65536))
   assert len(packets)==(2 if baseline else 1),(tui,len(packets))
   snapshot=saved();assert any('Keep original constraint' in str(m.get('content','')) for m in snapshot['messages']);assert list(work.iterdir())==[]
   send('/exit');p.wait(timeout=5)
   return {'surface':'TUI 40x24' if tui else 'CLI','requests':len(packets),'sent_after_http_429':True,'narrowed_scope_persisted':saved().get('scope',{}).get('grants')==[],'old_constraint_preserved':True,'cancelled_current_turn':cancelled,'draft_resubmitted_without_retyping':tui,'baseline':baseline,'passed':True}
  finally:
   if p.poll() is None:p.kill();p.wait()
   if master is not None:os.close(master);os.close(slave)
   server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args()
 print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[trial(a.binary.resolve(),t,a.expect_regression) for t in ([False,True] if os.name=='posix' else [False])],'limits':'Local scripted HTTP, actual CLI and POSIX phone-sized PTY. Not Android. CLI returns to input rather than retrying unread instructions; TUI cancels then requires Enter to submit the retained draft.'},indent=2))
