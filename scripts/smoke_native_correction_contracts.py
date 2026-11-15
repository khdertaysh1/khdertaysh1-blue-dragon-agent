#!/usr/bin/env python3
"""Native missing-ok contracts through real CLI; no model-authored success oracle."""
import argparse,hashlib,http.server,json,os,subprocess,tempfile,threading
from pathlib import Path
TOOLS={'browser':{'action':'status'},'search_file':{'path':'app.js','query':'needle','byte_offset':0,'scan_bytes':1024,'max_matches':5},'list_files':{'path':'.'},'read_lines':{'path':'app.js','start':1,'count':2},'file_info':{'path':'app.js'},'search_files':{'query':'needle'},'file_outline':{'path':'app.js'},'project_map':{},'git_status':{},'git_diff':{},'list_jobs':{},'read_file':{'path':'app.js'},'read_file_window':{'path':'app.js','byte_offset':0,'byte_count':64}}
def run(binary,tool,mode,baseline=False):
 with tempfile.TemporaryDirectory(prefix='blue-native-contract-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir();data=b'const needle = 1;\nfunction found() { return needle; }\n';(work/'app.js').write_bytes(data);(work/'failed-name.js').write_text('const still_data = true;');(work/'.env').write_text('PRIVATE_NATIVE_SECRET')
  if mode!='git_failure':subprocess.run(['git','init','-q',str(work)],check=True,capture_output=True)
  packets=[]
  good=dict(TOOLS[tool]);bad=dict(good)
  if 'action' in bad:bad['action']=42
  elif 'path' in bad:bad['path']=42
  elif 'query' in bad:bad['query']=42
  else:bad['unexpected']=42
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    p=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(p);i=len(packets);m={'role':'assistant','content':'Native results accounted for.'}
    if i<=2:
     name=tool;args=dict(bad if i==1 else good)
     if i==2 and mode=='other':name='read_file_window' if tool!='read_file_window' else 'file_info';args=dict(TOOLS[name])
     if i==2 and mode in ('missing','denied'):args['path']='missing.js' if mode=='missing' else '.env'
     m.update(content=None,tool_calls=[{'id':f'c{i}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}])
    body=json.dumps({'choices':[{'message':m,'finish_reason':'tool_calls' if i<=2 else 'stop'}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP')};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
  try:
   p=subprocess.run([str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','4','run','Perform only the scripted local tool check. Preserve files.'],env=env,text=True,capture_output=True,timeout=25)
   s=json.loads(next((root/'state/sessions').glob('*.json')).read_text());r=[json.loads(m['content']) for m in s['messages'] if m['role']=='tool'];assert len(packets)==3 and len(r)==2 and r[0]['category']=='invalid_arguments'
   assert (work/'app.js').read_bytes()==data and 'PRIVATE_NATIVE_SECRET' not in json.dumps(r)
   wanted=mode=='success' and not baseline
   assert (p.returncode==0)==wanted,(tool,mode,p.returncode,r,p.stdout,p.stderr)
   assert s['last_report']['finished']==wanted
   if mode=='success':
    v=r[1];assert 'ok' not in v,(tool,v)
    if tool=='browser':assert v['backend']=='none' and v['ready'] is False
    if tool=='search_file':assert v['whole_file_searched'] is True and [m['byte_offset'] for m in v['matches']]==[6,44]
    if tool=='list_files':assert 'app.js' in v['files'] and '.env' not in v['files']
    if tool=='read_lines':assert [x['text'] for x in v['lines']]==data.decode().splitlines()
    if tool=='file_info':assert v['bytes']==len(data) and v['directory'] is False
    if tool=='search_files':assert [x['line'] for x in v['matches'] if x['path']=='app.js']==[1,2]
    if tool=='file_outline':assert v['source_sha256']==hashlib.sha256(data).hexdigest() and any(x['name']=='found' for x in v['items'])
    if tool=='project_map':assert v['files_inspected']==2
    if tool=='git_status':assert 'failed-name.js' in v['status']
    if tool=='git_diff':assert v['diff']==''
    if tool=='list_jobs':assert v['jobs']==[]
   if mode in ('missing','denied','git_failure'):assert r[1]['ok'] is False
   return {'tool':tool,'mode':mode,'exit_code':p.returncode,'expected_success':wanted,'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args()
 cases=[(t,'success') for t in ('search_file','list_files','read_lines','file_info','browser')] if a.expect_regression else [(t,m) for t in TOOLS for m in ('success','other')]+[(t,m) for t in ('search_file','list_files','read_lines','file_info') for m in ('missing','denied')]+[('git_diff','git_failure'),('git_status','git_failure')]
 print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[run(a.binary.resolve(),t,m,a.expect_regression) for t,m in cases],'limits':'Scripted provider with real native CLI/filesystem/git; independent file/result checks. Direct Rust controls cover forged flags/contracts and non-dispatched provenance. Not phone or live-model qualification.'},indent=2))
