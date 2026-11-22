#!/usr/bin/env python3
"""Actual CLI + scripted provider; never runs the reported external curl request."""
import argparse,hashlib,http.server,json,os,subprocess,sys,tempfile,threading
from pathlib import Path
ORIGINAL={'program':'curl','args':'["-sS", "-I", "--connect-timeout", "10", "--max-time", "20", "https://flatscout.co.uk"]','timeout_seconds':'30'}
def run(binary, mode):
 with tempfile.TemporaryDirectory(prefix='blue-command-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir(); packets=[];errors=[]
  local={'program':sys.executable,'args':json.dumps(['-c','from pathlib import Path; Path("ran.txt").open("a").write("ok"); print("LOCAL_OK")']),'timeout_seconds':'30'}
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    try:
     p=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(p);index=len(packets)
     if index==1:
      name='discover_tools';args={'names':['"run_command"' if mode.startswith('quoted_') else 'run_command']}
     elif index==2 or (mode in ('repeat','correct') and index==3):
      schema=next(d['function']['parameters'] for d in p['tools'] if d['function']['name']=='run_command')
      assert schema['properties']['args']['type']=='array' and schema['properties']['timeout_seconds']['type']=='integer'
      name='run_command';args=ORIGINAL if mode=='original_denied' else local
      if mode in ('repeat','invalid_final') or (mode=='correct' and index==2):args={'args':[],'timeout_seconds':'30'}
      if mode=='spawn':args={'program':'__blue_missing_program__','args':[],'timeout_seconds':30}
      if mode=='nonzero':args={'program':sys.executable,'args':['-c','raise SystemExit(7)'],'timeout_seconds':30}
     else:name=None
     message={'role':'assistant','content':'Native result reviewed.'}
     if name:message={'role':'assistant','content':None,'tool_calls':[{'id':f'c{index}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
     if mode in ('stream','truncated') and name:
      raw=message['tool_calls'][0]['function']['arguments'];events=[]
      if mode=='truncated' and name=='run_command':raw=raw[:len(raw)//2]
      for i in range(0,len(raw),9):
       call={'index':0,'function':{'arguments':raw[i:i+9]}}
       if i==0:call.update(id=f'c{index}',type='function');call['function']['name']=name
       events.append({'choices':[{'delta':{'tool_calls':[call]}}]})
      events.append({'choices':[{'delta':{},'finish_reason':'length' if mode=='truncated' and name=='run_command' else 'tool_calls'}]})
      data=''.join('data: '+json.dumps(e)+'\n\n' for e in events).encode();mime='text/event-stream'
     elif mode in ('stream','truncated'):data=('data: '+json.dumps({'choices':[{'delta':{'content':message['content']},'finish_reason':'stop'}]})+'\n\n').encode();mime='text/event-stream'
     else:data=json.dumps({'choices':[{'message':message,'finish_reason':'tool_calls' if name else 'stop'}]}).encode();mime='application/json'
     self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    except Exception as e:errors.append(str(e));self.send_error(500)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
  if mode=='projected':env['BLUE_DRAGON_INPUT_BUDGET']='4096'
  command=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-rotation','--max-steps','5']
  if mode not in ('stream','truncated'):command+=['--no-stream']
  try:
   consent='' if mode in ('repeat','invalid_final','truncated') else 'n\n' if mode in ('denied','original_denied','quoted_denied') else 'y\n'
   if mode.startswith('tui_'):
    import fcntl,pty,select,struct,termios,time
    from pty_helpers import contains_styled_text
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,40,0,0))
    env['TERM']='xterm-256color';child=subprocess.Popen(command+['tui'],stdin=slave,stdout=slave,stderr=slave,env=env);captured=bytearray()
    def wait_text(needle):
     until=time.monotonic()+15
     while time.monotonic()<until:
      if contains_styled_text(captured,needle):return
      if select.select([master],[],[],0.05)[0]:captured.extend(os.read(master,65536))
     raise AssertionError(('phone-sized TUI missing',needle,bytes(captured[-2000:])))
    try:
     wait_text(b'BLUE DRAGON');os.write(master,'نفّذ المهمة المحلية فقط / Run local fixture only.\r'.encode());wait_text(b'APPROVAL REQUIRED')
     os.write(master,b'n' if mode=='tui_denied' else b'y')
     wait_text(b'Denied by user' if mode=='tui_denied' else b'Completed')
     # Wait for a model final response, then close without another provider task.
     until=time.monotonic()+10
     while len(packets)<3 and time.monotonic()<until:
      if select.select([master],[],[],0.05)[0]:captured.extend(os.read(master,65536))
     time.sleep(0.3);os.write(master,b'/exit\r');child.wait(timeout=8)
     p=subprocess.CompletedProcess([],child.returncode,bytes(captured).decode(errors='replace'),'')
    finally:
     if child.poll() is None:child.kill();child.wait()
     os.close(master);os.close(slave)
   else:p=subprocess.run(command+['chat'],input='Run the local fixture only.\n'+consent+'/exit\n',env=env,text=True,capture_output=True,timeout=35)
   assert not errors,errors
   path=next((root/'state/sessions').glob('*.json'));session=json.loads(path.read_text());results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool']
   assert results[0].get('ok') is True,(mode,len(packets),results,p.stdout,p.stderr)
   if mode.startswith('quoted_'):
    assert results[0]['name_normalizations']==[{'original':'"run_command"','effective':'run_command','operation':'single_json_string_decode','permission_changed':False}]
    assert results[0]['permission_changed'] is False and len(packets) in ({3,4} if mode=='quoted_approve' else {3}),(mode,len(packets),results,p.stdout,p.stderr)
    assert len(results)==2 and 'run_command' in results[0]['active_schema_names']
    first=next(m for m in session['messages'] if m.get('tool_calls'))
    assert json.loads(first['tool_calls'][0]['function']['arguments'])=={'names':['"run_command"']}
   commands=[r for r in results if 'command_outcome' in r or r.get('category')=='invalid_arguments' or r.get('repetition_blocked')]
   if mode=='truncated':
    assert not commands and not (work/'ran.txt').exists() and 'APPROVAL REQUIRED' not in p.stdout
    assert len(packets)==2 and session['last_report']['finished'] is False
    return {'case':mode,'requests':len(packets),'passed':True}
   assert commands,(mode,p.stderr,p.stdout)
   if mode=='projected':assert session['last_report']['context']['projection_level']>0,session['last_report']['context']
   if mode=='invalid_final':
    assert not (work/'ran.txt').exists() and session['last_report']['finished'] is False
    assert session['last_report']['task_status']=='needs_verification' and 'APPROVAL REQUIRED' not in p.stdout
    return {'case':mode,'requests':len(packets),'passed':True}
   if mode=='repeat':
    assert not (work/'ran.txt').exists() and len(packets)==3,(len(packets),commands,p.stderr,p.stdout)
    assert commands[0]['category']=='invalid_arguments' and commands[-1]['repetition_blocked'] is True
    assert 'APPROVAL REQUIRED' not in p.stdout
   else:
    result=commands[-1];assert session['last_report']['finished'] is True,session['last_report']
    assert 'APPROVAL REQUIRED' in p.stdout,(mode,p.stdout,p.stderr)
    if mode in ('denied','original_denied','tui_denied','quoted_denied'):assert result['command_outcome']=='denied' and not(work/'ran.txt').exists()
    elif mode=='spawn':assert result['command_outcome']=='spawn_failure'
    elif mode=='nonzero':assert result['command_outcome']=='nonzero_exit' and result['exit_code']==7
    else:assert result['stdout']=='LOCAL_OK'+os.linesep and result['exit_code']==0 and (work/'ran.txt').read_text()=='ok'
    if mode not in ('spawn','nonzero'):
     assert len(result['argument_normalization']['changes'])==2
     assert isinstance(result['argument_normalization']['effective_arguments']['args'],list)
     native=next(m for m in session['messages'] if m.get('tool_calls') and m['tool_calls'][0]['function']['name']=='run_command' and 'program' in json.loads(m['tool_calls'][0]['function']['arguments']))
     assert isinstance(json.loads(native['tool_calls'][0]['function']['arguments'])['args'],str)
    count=len(packets);evidence=result['evidence_id']
    inspect=subprocess.run(command+['--resume','latest','chat'],input=f'/tool-result {evidence}\n/exit\n',env=env,text=True,capture_output=True,timeout=10)
    assert len(packets)==count and '"actions_replayed": 0' in inspect.stdout
    assert 'flatscout' not in inspect.stdout and 'ran.txt' not in inspect.stdout and 'LOCAL_OK' not in inspect.stdout
   return {'case':mode,'requests':len(packets),'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);a=p.parse_args()
 print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[run(a.binary.resolve(),m) for m in ('quoted_approve','quoted_denied','original_denied','approve','projected','denied','repeat','invalid_final','correct','spawn','nonzero','stream','truncated')+ (('tui_approve','tui_denied') if os.name=='posix' else ())],'limits':'Linux/local scripted provider, real subprocess and approvals. External curl fixture was denied, never executed. Not physical Termux or live provider.'},indent=2))
