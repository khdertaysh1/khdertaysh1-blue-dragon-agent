"""Real controlling PTY, local scripted provider; no live model or site."""
import argparse,http.server,json,os,pathlib,pty,fcntl,termios,struct,subprocess,tempfile,threading,time,select,hashlib
from pty_helpers import contains_styled_text

def trial(binary,interface,decision='y',long=False,stale=False,edit_arabic=False):
 with tempfile.TemporaryDirectory(prefix='bd-input-') as td:
  root=pathlib.Path(td);work=root/'work';work.mkdir();home=root/'state';packets=[]
  class Provider(http.server.BaseHTTPRequestHandler):
   def log_message(self,*a):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q)
    if any(m['role']=='tool' for m in q['messages']):m={'role':'assistant','content':'Native result recorded; no further actions.'}
    else:
     time.sleep(.2)
     code='from pathlib import Path; Path("executed").write_text("once"); print("BLUE_DRAGON_OK")'
     m={'role':'assistant','content':None,'tool_calls':[{'id':'once','type':'function','function':{'name':'run_command','arguments':json.dumps({'program':'python3','args':['-c',code],'timeout_seconds':10})}}]}
    b=json.dumps({'choices':[{'message':m,'finish_reason':'tool_calls' if m.get('tool_calls') else 'stop'}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=server.serve_forever,daemon=True).start()
  master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,60,0,0));before=termios.tcgetattr(slave)
  env={k:v for k,v in os.environ.items() if k in ['PATH','TMP','TEMP']};env.update(HOME=td,BLUE_DRAGON_HOME=str(home),TERM='xterm-256color',NO_COLOR='1')
  def controlling():os.setsid();fcntl.ioctl(0,termios.TIOCSCTTY,0)
  proc=subprocess.Popen([str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation',interface],stdin=slave,stdout=slave,stderr=slave,env=env,preexec_fn=controlling)
  out=bytearray()
  def pump(seconds=.1):
   end=time.monotonic()+seconds
   while time.monotonic()<end:
    if select.select([master],[],[],.02)[0]:
     try:out.extend(os.read(master,65536))
     except OSError:break
  def wait(predicate):
   end=time.monotonic()+20
   while time.monotonic()<end:
    if predicate():return
    pump(.05)
   raise AssertionError({'interface':interface,'decision':decision,'tail':bytes(out[-2000:]).decode(errors='replace')})
  try:
   wait(lambda:b'blue-dragon>' in out if interface=='chat' else contains_styled_text(out,b'BLUE DRAGON'))
   task='Print BLUE_DRAGON_OK once after my approval.\nلا تستخدم بديلاً إذا رفضت.\nTreat this entire pasted request as one task.'
   if long:task+='\n'+(('تفصيل عربي English '+'x'*50+' ')*85).rstrip()
   if edit_arabic:
    # Type a marked Arabic word, erase the final grapheme, move right in RTL, then insert.
    os.write(master,'سَلامُ'.encode()+b'\x7f\x1b[C'+'م'.encode()+b'\r')
    task='سَلما'
   else:
    os.write(master,b'\x1b[200~'+task.encode()+b'\x1b[201~\r'+(b'y\r' if stale else b''))
   wait(lambda:b'APPROVAL REQUIRED' in out if interface=='chat' else contains_styled_text(out,b'APPROVAL'))
   pump(.3);assert len(packets)==1 and not (work/'executed').exists(),'No execution or approval from stale input'
   assert task in [m.get('content') for m in packets[0]['messages'] if m['role']=='user']
   os.write(master,b'\r');pump(.2);assert len(packets)==1 and not (work/'executed').exists(),'Enter must wait'
   os.write(master,{'y':b'y','n':b'n','eof':b'\x04','cancel':b'\x03'}[decision])
   if decision in ['y','n','eof']:wait(lambda:len(packets)==2)
   else:pump(1)
   wait(lambda:len(list((home/'sessions').glob('*.json')))>0);pump(.2)
   s=json.loads(next((home/'sessions').glob('*.json')).read_text());rs=[json.loads(m['content']) for m in s['messages'] if m['role']=='tool'];assert len(rs)==1,rs
   if decision=='y':assert (work/'executed').read_text()=='once' and rs[0]['stdout']=='BLUE_DRAGON_OK\n' and rs[0]['exit_code']==0
   else:
    assert not (work/'executed').exists()
    if decision=='n':assert rs[0]['denied'] is True and rs[0]['approval_source']=='user_choice',rs
    if decision=='eof':assert rs[0]['category']=='approval_eof',rs
    if decision=='cancel':assert rs[0].get('category')=='approval_cancelled',rs
   if interface=='chat':
    if stale:os.write(master,b'\x03');pump(.2)
    os.write(master,b'/exit\r')
   else:
    if stale:os.write(master,b'\x15')
    os.write(master,b'\x03')
   wait(lambda:proc.poll() is not None);assert termios.tcgetattr(slave)==before
   return {'interface':interface,'decision':decision,'long_input':long,'stale_keys':stale,'arabic_keyboard_edit':edit_arabic,'passed':True,'provider_requests':len(packets),'execution_count':int(decision=='y'),'one_complete_task':True,'terminal_restored':True}
  finally:
   if proc.poll() is None:proc.terminate();proc.wait(timeout=5)
   os.close(master);os.close(slave);server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('binary',type=pathlib.Path);a=p.parse_args();b=a.binary.resolve()
 cases=[(i,d,l,s) for i in ['chat','tui'] for d,l,s in [('y',False,False),('y',True,False),('n',False,False),('n',False,True),('cancel',False,False)]]+[('chat','eof',False,False),('tui','y',False,False,True)]
 rows=[]
 for c in cases:rows.append(trial(b,*c))
 print(json.dumps({'binary_sha256':hashlib.sha256(b.read_bytes()).hexdigest(),'scripted_provider':True,'real_pty':True,'rows':rows},indent=2))
