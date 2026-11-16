#!/usr/bin/env python3
"""CLI regression of reported event shape; original phone arguments unavailable.
Uses real curl HEAD to loopback, never the user's external site.
"""
import argparse, hashlib, http.server, json, os, subprocess, tempfile, threading
from pathlib import Path

def run(binary, mode, baseline=False):
 with tempfile.TemporaryDirectory(prefix='blue-diagnostics-') as folder:
  root=Path(folder);work=root/'project';work.mkdir();packets=[];hits=[]
  class Server(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_HEAD(self):
    hits.append(self.path);self.send_response(200);self.send_header('X-Blue-Fixture','diagnostic-recovery');self.end_headers()
   def do_POST(self):
    p=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(p);i=len(packets)
    def call(name,args,n):return {'id':f'c{i}-{n}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}
    calls=[]
    if i==1:calls=[call('run_command',{'args':[]},0)]
    elif i==2:calls=[call('discover_tools',{'names':['run_command']},0)]
    elif i==3 and mode!='discovery_only':calls=[call('discover_tools',{'names':[] if mode!='large' else ['run_command']*9},0)]
    elif i==4 and mode!='discovery_only':
     assert any(d['function']['name']=='run_command' for d in p['tools'])
     calls=[call('discover_tools',{'names':['run_command','read_lines']},0),call('run_command',{'program':'curl','args':['-sS','-I','--connect-timeout','10','--max-time','20',f'http://127.0.0.1:{self.server.server_port}/head'],'timeout_seconds':30},1)]
    message={'role':'assistant','content':'Native results only.'}
    if calls:message.update(content=None,tool_calls=calls)
    data=json.dumps({'choices':[{'message':message,'finish_reason':'tool_calls' if calls else 'stop'}]}).encode()
    self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Server);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
  args=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','5']
  try:
   p=subprocess.run(args+['run','Execute the supplied local curl fixture only; require approval.'],input='n\n' if mode=='denied' else 'y\n',env=env,text=True,capture_output=True,timeout=30)
   s=json.loads(next((root/'state/sessions').glob('*.json')).read_text());results=[json.loads(m['content']) for m in s['messages'] if m['role']=='tool']
   assert results[0]['category']=='invalid_arguments' and results[1]['ok'] is True
   if baseline:
    assert mode in ('empty','large') and len(packets)==3 and len(results)==3 and not hits
    assert results[2]['category']=='invalid_arguments' and 'actual_count' not in results[2]
    assert p.returncode!=0 and 'APPROVAL REQUIRED' not in p.stdout
    return {'mode':mode,'requests':len(packets),'head_requests':0,'baseline_global_stop_reproduced':True}
   if mode=='discovery_only':
    assert not hits and p.returncode!=0 and s['last_report']['finished'] is False
    assert 'APPROVAL REQUIRED' not in p.stdout
   else:
    e=results[2];assert e['actual_count']==(9 if mode=='large' else 0) and e['minItems']==1 and e['maxItems']==8,e
    assert e['correction_attempt']==1 and results[0]['correction_attempt']==1
    assert results[3]['ok'] is True and len(packets)==5
    assert 'APPROVAL REQUIRED' in p.stdout
    if mode=='denied':assert not hits and results[4]['command_outcome']=='denied' and s['last_report']['finished'] is False
    else:
     assert hits==['/head'] and results[4]['exit_code']==0 and results[4]['ok'] is True
     assert 'X-Blue-Fixture: diagnostic-recovery' in results[4]['stdout'] and p.returncode==0
   for index in ([0] if mode=='discovery_only' else [0,2]):
    count=len(packets);before=list(hits);evidence=results[index]['evidence_id']
    view=subprocess.run(args+['--resume','latest','chat'],input=f'/tool-result {evidence}\n/exit\n',env=env,text=True,capture_output=True,timeout=10)
    assert len(packets)==count and hits==before
    assert '"category": "invalid_arguments"' in view.stdout and '"execution_status": "not_executed"' in view.stdout
    assert '"actual_type":' in view.stdout and 'schema_validation_failed' in view.stdout
    if index==2:
     assert f'"actual_count": {9 if mode=="large" else 0}' in view.stdout and '"maxItems": 8' in view.stdout
     assert '"field": "$.names"' in view.stdout and '"actual_type": "array"' in view.stdout
     assert '"expected_type": "advertised size bounds"' in view.stdout and '"original_argument_shape"' in view.stdout
   return {'mode':mode,'requests':len(packets),'head_requests':len(hits),'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args()
 print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[run(a.binary.resolve(),m,a.expect_regression) for m in (('empty','large') if a.expect_regression else ('empty','large','discovery_only','denied'))],'limits':'Hypothesized argument fixtures for the reported event sequence, not an exact reproduction of unavailable phone arguments. Real curl/approval/loopback on Linux; scripted model, not live-model or phone proof.'},indent=2))
