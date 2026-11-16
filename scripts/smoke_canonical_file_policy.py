#!/usr/bin/env python3
"""Native CLI alias policy: synthetic secrets never reach provider on rejection."""
import argparse,hashlib,http.server,json,os,subprocess,tempfile,threading
from pathlib import Path
SECRET='PRIVATE_NATIVE_SECRET'
TOOLS={'read_file':{},'read_lines':{'start':1,'count':2},'read_file_window':{'byte_offset':0,'byte_count':64},'search_file':{'query':'PRIVATE','byte_offset':0,'scan_bytes':1024,'max_matches':5},'file_outline':{},'file_info':{}}
def run(binary,tool,kind,baseline):
 with tempfile.TemporaryDirectory(prefix='blue-alias-policy-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir();outside=root/'outside';outside.mkdir();(outside/'data.txt').write_text(SECRET);(work/'.env').write_text(SECRET);(work/'.ssh').mkdir();(work/'.ssh/data.txt').write_text(SECRET);(work/'allowed.txt').write_text('public text\n');(work/'safe-dir').mkdir();(work/'safe-dir/data.txt').write_text('public text\n')
  os.symlink('.env',work/'alias.txt');os.symlink('.ssh',work/'alias-dir');os.symlink(outside,work/'outside-link');os.symlink('allowed.txt',work/'safe.txt');os.symlink('safe-dir',work/'safe-dir-link')
  path={'secret':'alias.txt','directory':'alias-dir/data.txt','outside':'outside-link/data.txt','allowed':'safe.txt'}[kind]
  if tool=='list_files':path={'secret':'alias-dir','directory':'alias-dir','outside':'outside-link','allowed':'safe-dir-link'}[kind]
  packets=[]
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))));n=len(packets);m={'role':'assistant','content':'Native operation accounted for; no new claims.'}
    if n<3:
     args=dict(TOOLS.get(tool,{}));args['path']=42 if n==1 else path
     m.update(content=None,tool_calls=[{'id':f'c{n}','type':'function','function':{'name':tool,'arguments':json.dumps(args)}}])
    body=json.dumps({'choices':[{'message':m,'finish_reason':'tool_calls' if n<3 else 'stop'}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','TMP','TEMP')};env.update(HOME=tmp,BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
  try:
   p=subprocess.run([str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','4','run','Only perform the scripted local tool operation.'],text=True,capture_output=True,env=env,timeout=25)
   saved=json.loads(next((root/'state/sessions').glob('*.json')).read_text());results=[json.loads(m['content']) for m in saved['messages'] if m['role']=='tool'];assert len(packets)==3 and len(results)==2 and results[0]['category']=='invalid_arguments'
   leaked=any(SECRET in value for value in [json.dumps(packets),json.dumps(saved),p.stdout,p.stderr])
   if baseline:assert leaked and p.returncode==0
   else:
    assert not leaked,(tool,kind,'synthetic secret escaped')
    if kind=='allowed':assert p.returncode==0 and results[1].get('ok') is not False,(tool,results)
    else:assert p.returncode!=0 and results[1]['ok'] is False,(tool,results)
   assert (work/'.env').read_text()==SECRET and (outside/'data.txt').read_text()==SECRET
   return {'tool':tool,'case':kind,'exit_code':p.returncode,'synthetic_secret_transmitted':leaked,'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args()
 if os.name!='posix':print(json.dumps({'status':'not_run','reason':'POSIX symlink fixture; Windows symlink privileges unavailable'}))
 else:
  cases=[(t,'secret') for t in ('read_file','read_lines')] if a.expect_regression else [(t,k) for t in [*TOOLS,'list_files'] for k in ('allowed','secret','directory','outside')]
  print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[run(a.binary.resolve(),t,k,a.expect_regression) for t,k in cases],'limits':'Real Linux CLI/native filesystem, scripted provider, synthetic secret only. No hostile concurrent filesystem writer, Windows/macOS or actual Termux test.'},indent=2))
