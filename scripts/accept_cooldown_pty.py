"""Local scripted 429, real CLI PTY; fixed deadline and explicit resume, no live service."""
import pathlib,sys,http.server,json,threading,os,pty,subprocess,tempfile,time,select,fcntl,termios,hashlib
b=pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='bd-cooldown-') as td:
 root=pathlib.Path(td);work=root/'work';work.mkdir();packets=[]
 class P(http.server.BaseHTTPRequestHandler):
  def log_message(self,*a):pass
  def do_POST(self):
   q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q)
   if len(packets)==2:
    v={'error':{'code':'tpm_rate_limit_exceeded','message':'Synthetic shared capacity'}};status=429
   else:
    m={'role':'assistant','content':'Saved result retained; no further action.'}
    if len(packets)==1:m={'role':'assistant','content':None,'tool_calls':[{'id':'one','type':'function','function':{'name':'run_command','arguments':json.dumps({'program':'python3','args':['-c','from pathlib import Path; Path("once").write_text("ok"); print("DONE")'],'timeout_seconds':5})}}]}
    v={'choices':[{'message':m,'finish_reason':'tool_calls' if m.get('tool_calls') else 'stop'}]};status=200
   data=json.dumps(v).encode();self.send_response(status)
   if status==429:self.send_header('Retry-After','5')
   self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),P);threading.Thread(target=server.serve_forever,daemon=True).start()
 master,slave=pty.openpty();out=bytearray();env={k:v for k,v in os.environ.items() if k=='PATH'};env.update(HOME=td,BLUE_DRAGON_HOME=str(root/'state'),TERM='xterm',NO_COLOR='1')
 def setup():os.setsid();fcntl.ioctl(0,termios.TIOCSCTTY,0)
 proc=subprocess.Popen([str(b),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','chat'],stdin=slave,stdout=slave,stderr=slave,env=env,preexec_fn=setup)
 def pump(t=.1):
  end=time.monotonic()+t
  while time.monotonic()<end:
   if select.select([master],[],[],.02)[0]:
    try:out.extend(os.read(master,65536))
    except OSError:return
 def wait(f):
  end=time.monotonic()+20
  while not f():
   pump()
   if time.monotonic()>end:raise AssertionError(bytes(out[-2500:]))
 def session():return json.loads(next((root/'state/sessions').glob('*.json')).read_text())
 try:
  wait(lambda:b'blue-dragon>' in out);os.write(master,b'Print DONE once with my approval, then report.\r');wait(lambda:b'Approve this ONE action?' in out);os.write(master,b'y');wait(lambda:len(packets)==2);pump(.4)
  files=list((root/'state').rglob('*.json'));budget=next(p for p in files if json.loads(p.read_text()).get('schema')==1 and 'until' in json.loads(p.read_text()));deadline=json.loads(budget.read_text())['until'];assert deadline>time.time()-1
  start=len(out)
  for msg in ['Keep everything read-only now.','لا تعِد تنفيذ الأمر.']:
   os.write(master,msg.encode()+b'\r');pump(.25)
  assert len(packets)==2;assert json.loads(budget.read_text())['until']==deadline
  current=session();texts=[m.get('content') for m in current['messages'] if m['role']=='user'];assert all(m in texts for m in ['Keep everything read-only now.','لا تعِد تنفيذ الأمر.'])
  assert bytes(out[start:]).count(b'Provider cooldown until Unix')==1
  while time.time()<=deadline:pump(.1)
  os.write(master,b'Continue from saved results, do not execute again.\r');wait(lambda:len(packets)==3);pump(.3)
  results=[json.loads(m['content']) for m in session()['messages'] if m['role']=='tool'];assert len(results)==1 and results[0]['stdout']=='DONE\n' and results[0]['exit_code']==0;assert (work/'once').read_text()=='ok'
  assert all(m in str(packets[-1]) for m in ['Keep everything read-only now.','لا تعِد تنفيذ الأمر.'])
  os.write(master,b'/exit\r');wait(lambda:proc.poll() is not None)
  print(json.dumps({'passed':True,'binary_sha256':hashlib.sha256(b.read_bytes()).hexdigest(),'provider_requests':3,'executions':1,'deadline_unchanged':True,'new_instructions_preserved':True,'resume':'explicit user turn after fixed deadline','live_provider':False}))
 finally:
  if proc.poll() is None:proc.terminate();proc.wait(timeout=5)
  os.close(master);os.close(slave);server.shutdown();server.server_close()
