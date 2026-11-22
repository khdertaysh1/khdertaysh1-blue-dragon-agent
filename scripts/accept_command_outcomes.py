"""Run native CLI commands with synthetic provider, assert original outcome fields."""
import pathlib,sys,tempfile,os,subprocess,http.server,json,threading,hashlib
b=pathlib.Path(sys.argv[1]).resolve();rows=[]
for case in ['nonzero','spawn_failure','timeout','eof','invalid_input','normalized','broken_json']:
 with tempfile.TemporaryDirectory() as td:
  root=pathlib.Path(td);packets=[]
  args={'program':'python3','args':['-c','print("OUT"); import sys; print("ERR",file=sys.stderr); sys.exit(7)'],'timeout_seconds':5}
  if case=='spawn_failure':args={'program':'bd_program_does_not_exist_942','args':[],'timeout_seconds':5}
  if case=='timeout':args={'program':'python3','args':['-c','import time; print("START",flush=True); time.sleep(10)'],'timeout_seconds':1}
  if case in ['normalized','broken_json']:args={'program':'printf','args':'["NORMALIZED\\n"]' if case=='normalized' else '["broken"','timeout_seconds':'5'}
  class P(http.server.BaseHTTPRequestHandler):
   def log_message(self,*a):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q);m={'role':'assistant','content':'Report the recorded outcome only.'}
    if len(packets)==1:m={'role':'assistant','content':None,'tool_calls':[{'id':'c','type':'function','function':{'name':'run_command','arguments':json.dumps(args)}}]}
    data=json.dumps({'choices':[{'message':m,'finish_reason':'tool_calls' if m.get('tool_calls') else 'stop'}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),P);threading.Thread(target=server.serve_forever,daemon=True).start();env={'PATH':os.environ['PATH'],'HOME':td,'BLUE_DRAGON_HOME':str(root/'state'),'NO_COLOR':'1'}
  try:
   p=subprocess.run([str(b),'--root',td,'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','run','Execute a local command only with my approval and report its actual result.'],input='' if case=='eof' else 'not-a-choice\n' if case=='invalid_input' else 'y\n',text=True,capture_output=True,env=env,timeout=25)
   s=json.loads(next((root/'state/sessions').glob('*.json')).read_text());r=[json.loads(m['content']) for m in s['messages'] if m['role']=='tool'][0]
   expected={'nonzero':'nonzero_exit','spawn_failure':'spawn_failure','timeout':'timeout','eof':'approval_eof','invalid_input':'approval_invalid_input','broken_json':'invalid_arguments'}
   if case=='normalized':assert r['stdout']=='NORMALIZED\n' and r['exit_code']==0,r
   else:assert r.get('category')==expected[case],(case,r)
   if case=='nonzero':assert r['exit_code']==7 and r['stdout']=='OUT\n' and r['stderr']=='ERR\n'
   if case=='timeout':assert r['execution_status']=='unknown' and 'START' in r['stdout']
   if case in ['spawn_failure','eof','invalid_input','broken_json']:assert r['execution_status']=='not_executed',r
   rows.append({'case':case,'passed':True,'category':r.get('category'),'requests':len(packets)})
  finally:server.shutdown();server.server_close()
print(json.dumps({'binary_sha256':hashlib.sha256(b.read_bytes()).hexdigest(),'scripted_provider':True,'rows':rows},indent=2))
