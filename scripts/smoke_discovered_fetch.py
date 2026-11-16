#!/usr/bin/env python3
"""Exact advertised-transition GET count and approval denial, loopback only."""
import hashlib,http.server,json,os,pathlib,subprocess,sys,tempfile,threading
from schema_fixture_support import fixture_env, require_progress
binary=pathlib.Path(sys.argv[1]).resolve();baseline='--expect-regression' in sys.argv[2:]
for approve in (True,False):
 with tempfile.TemporaryDirectory(prefix='blue-schema-fetch-') as d:
  root=pathlib.Path(d);(root/'work').mkdir();packets=[];gets=[];dispatch_counts=[]
  class H(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_GET(self):
    gets.append(self.path);body=b'LOCAL_NATIVE_FETCH_EVIDENCE';self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q);dispatch_counts.append(len(gets));n=len(packets)
    if n<=4:
     name,args=('discover_tools',{'names':['web_fetch']}) if n==2 else ('web_fetch',{'url':url})
     msg={'role':'assistant','tool_calls':[{'id':f'c{n}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]};stop='tool_calls'
    else:msg={'role':'assistant','content':'No security conclusion. Local fixture response only.'};stop='stop'
    body=json.dumps({'choices':[{'message':msg,'finish_reason':stop}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),H);url=f'http://127.0.0.1:{server.server_port}/fixture';threading.Thread(target=server.serve_forever,daemon=True).start()
  env=fixture_env(root)
  run=subprocess.run([str(binary),'--root',str(root/'work'),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','6','run','Inspect only the explicitly approved local fixture; no external connections.'],input='y\n' if approve else 'n\n',env=env,text=True,encoding="utf-8",errors="replace",capture_output=True,timeout=40)
  server.shutdown();server.server_close()
  sessions=list((root/'state/sessions').glob('*.json'));require_progress(run,packets,4,sessions)
  state=json.loads(sessions[0].read_text(encoding='utf-8'));results=[json.loads(m['content']) for m in state['messages'] if m['role']=='tool']
  require_progress(run,packets,4,sessions,results,4)
  assert dispatch_counts[:3]==[0,0,0],dispatch_counts
  assert 'web_fetch' not in [v['function']['name'] for v in packets[0]['tools']]
  assert 'web_fetch' in [v['function']['name'] for v in packets[2]['tools']]
  assert results[0]['expected_type']=='advertised tool' and results[0]['execution_status']=='not_executed'
  assert results[1]['ok'] and not results[1]['permission_changed']
  if baseline:
   assert not gets and results[2]['repetition_blocked'] and run.returncode==1
  elif approve:
   assert gets==['/fixture'] and results[2]['status']==200 and results[3]['cached_result'] and run.returncode==0
   assert 'LOCAL_NATIVE_FETCH_EVIDENCE' in json.dumps(results[2])
  else:
   assert not gets and results[2]['status']=='denied by user' and results[3]['repetition_blocked'] and run.returncode==1
  print(json.dumps({'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'baseline':baseline,'approval':approve,'gets_before_advertisement':0,'native_get_count':len(gets),'same_call_after_discovery_blocked':results[2].get('repetition_blocked',False),'later_repeat_cached_or_blocked':bool(results[3].get('cached_result') or results[3].get('repetition_blocked')),'exit':run.returncode,'limits':'Loopback programmed provider and HTTP fixture; no external site, real browser or phone.'}))
