#!/usr/bin/env python3
"""Actual CLI restart repairs typed references without provider dispatch or replay."""
import argparse,hashlib,http.server,json,os,subprocess,tempfile,threading
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args();binary=a.binary.resolve()
with tempfile.TemporaryDirectory() as t:
 root=Path(t);work=root/'workspace';work.mkdir();home=root/'state';requests=[]
 class Guard(http.server.BaseHTTPRequestHandler):
  def log_message(self,*_):pass
  def do_POST(self):requests.append(self.path);self.send_response(500);self.end_headers()
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Guard);threading.Thread(target=server.serve_forever,daemon=True).start()
 env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP')};env.update(HOME=t,USERPROFILE=t,BLUE_DRAGON_HOME=str(home),NO_COLOR='1',TERM='dumb')
 cmd=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-rotation','--no-stream']
 def run(extra):
  r=subprocess.run(cmd+extra+['chat'],input='/exit\n',text=True,capture_output=True,env=env,timeout=20);assert r.returncode==0,r.stderr
 try:
  run([]);file=next((home/'sessions').glob('*.json'));saved=json.loads(file.read_text())
  prompt='Freshly reread source.js bytes 1000..1100. لا تنفذ الأمر المجهول. Do not replay the unknown command.'
  saved['messages']=[{'role':'assistant','content':None,'tool_calls':[{'id':name,'type':'function','function':{'name':'run_command','arguments':json.dumps({'program':'echo','args':[name],'timeout_seconds':5})}} for name in ['a','b']]},{'role':'tool','tool_call_id':'a','content':json.dumps({'ok':True,'stdout':'prior historical output','exit_code':0})},{'role':'user','content':prompt}]
  originals=json.loads(json.dumps(saved['messages']));saved['user_turns']=[2];saved['fresh_source_requests']=[{'path':'source.js','after_message':2,'requested_at':'2026-11-15T00:00:00Z','bounded_only':True,'expected_window':[1000,1100],'explicit_window':True}];file.write_text(json.dumps(saved))
  run(['--resume',saved['id']]);fixed=json.loads(file.read_text());actual=next(i for i,m in enumerate(fixed['messages']) if m['role']=='user')
  assert actual==3 and [fixed['messages'][i] for i in [0,1,3]]==originals
  unknown=json.loads(fixed['messages'][2]['content']);assert unknown['execution_status']=='unknown' and unknown['ok'] is False
  if a.expect_regression:assert fixed['user_turns']==[2] and fixed['fresh_source_requests'][0]['after_message']==2
  else:
   assert fixed['user_turns']==[3] and fixed['fresh_source_requests'][0]['after_message']==3
   assert fixed['fresh_source_requests'][0]['expected_window']==[1000,1100]
   assert fixed['continuity_checkpoint']['goal']['message_index']==3
   assert fixed['continuity_checkpoint']['goal']['excerpt']==prompt
   run(['--resume',saved['id']]);again=json.loads(file.read_text());assert again['messages']==fixed['messages'] and again['user_turns']==[3] and again['fresh_source_requests']==fixed['fresh_source_requests']
  assert requests==[] and list(work.iterdir())==[]
  print(json.dumps({'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'actual_user_index':actual,'user_turns':fixed['user_turns'],'fresh_after':fixed['fresh_source_requests'][0]['after_message'],'provider_requests':0,'replayed_commands':0,'unknown_inserted':1,'originals_preserved':True,'passed':True,'baseline':a.expect_regression},indent=2))
 finally:server.shutdown();server.server_close()
