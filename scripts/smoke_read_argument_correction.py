#!/usr/bin/env python3
"""Real CLI: corrected native reads without an ok flag resolve only their own error."""
import argparse,hashlib,http.server,json,os,subprocess,tempfile,threading
from pathlib import Path

def run(binary, tool, mode, baseline):
 with tempfile.TemporaryDirectory(prefix='blue-read-correction-') as folder:
  root=Path(folder);work=root/'project';work.mkdir();data=b'const corrected = true;\n';(work/'app.js').write_bytes(data);(work/'.env').write_text('PRIVATE_TEST_VALUE')
  packets=[]
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    p=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(p);i=len(packets)
    message={'role':'assistant','content':'Read result accounted for.'}
    if i<=2:
     name=tool
     path=42 if i==1 else '.env' if mode=='denied' else 'missing.js' if mode=='failed' else 'app.js'
     if i==2 and mode=='other_tool':name='read_file' if tool=='read_file_window' else 'read_file_window'
     args={'path':path}
     if name=='read_file_window':args.update(byte_offset=0,byte_count=64)
     message.update(content=None,tool_calls=[{'id':f'c{i}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}])
    body=json.dumps({'choices':[{'message':message,'finish_reason':'tool_calls' if i<=2 else 'stop'}]}).encode()
    self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
  try:
   p=subprocess.run([str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','4','run','Perform the scripted local check without changing files.'],env=env,text=True,capture_output=True,timeout=25)
   s=json.loads(next((root/'state/sessions').glob('*.json')).read_text());results=[json.loads(m['content']) for m in s['messages'] if m['role']=='tool']
   assert len(packets)==3 and len(results)==2 and results[0]['category']=='invalid_arguments'
   assert results[0]['field']=='$.path' and results[0]['actual_type']=='integer'
   assert (work/'app.js').read_bytes()==data and 'PRIVATE_TEST_VALUE' not in json.dumps(results)
   if mode in ('success','other_tool'):
    r=results[1];assert 'ok' not in r and r['content']==data.decode()
    assert r.get('sha256',r.get('window_sha256'))==hashlib.sha256(data).hexdigest()
   else:assert results[1]['ok'] is False
   should_pass=mode=='success' and not baseline
   assert (p.returncode==0)==should_pass,(tool,mode,p.returncode,p.stdout,p.stderr)
   assert s['last_report']['finished']==should_pass
   if not should_pass:assert 'Invalid tool arguments were not successfully corrected' in p.stderr+p.stdout
   return {'tool':tool,'mode':mode,'requests':3,'corrected_native_read':mode=='success','exit_code':p.returncode,'expected_success':should_pass,'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args()
 print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[run(a.binary.resolve(),tool,mode,a.expect_regression) for tool in ('read_file_window','read_file') for mode in (('success',) if a.expect_regression else ('success','other_tool','failed','denied'))],'limits':'Actual CLI/native filesystem with scripted provider. Stale/unknown/cached/hash-corrupt result rejection is tested directly in Rust, without adding a production fault-injection hook.'},indent=2))
