#!/usr/bin/env python3
"""A bounded native read followed by a default recall must reach the model intact.
Local scripted HTTP only; six-step cap unchanged, no command/browser execution.
"""
import argparse,hashlib,http.server,json,os,subprocess,tempfile,threading
from pathlib import Path

def trial(binary,baseline):
 with tempfile.TemporaryDirectory(prefix='blue-recall-view-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir();state=root/'state'
  source=(b'export function observed() { return "SOURCE_FACT_42"; }\n'+b'// synthetic padding\n'*5000);(work/'app.js').write_bytes(source)
  packets=[];exact=[];target=None;original=None
  class Provider(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    nonlocal target,original
    packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(packet)
    saved=json.loads(next((state/'sessions').glob('*.json')).read_text())
    results=[(i,m) for i,m in enumerate(saved['messages']) if m.get('tool_call_id')=='native_read']
    recalled=[m for m in saved['messages'] if m.get('tool_call_id')=='recall_original']
    if not results:
     advertised={x['function']['name'] for x in packet.get('tools',[])}
     name,args,ident=('read_file_window',{'path':'app.js','byte_offset':0,'byte_count':4096},'native_read') if 'read_file_window' in advertised else ('discover_tools',{'names':['read_file_window']},'discover')
    elif not recalled:
     target,original=results[0][0],results[0][1]['content']
     name,args,ident='recall_context',{'message_index':target},'recall_original'
    else:
     value=next((json.loads(m['content']) for m in packet['messages'] if m.get('tool_call_id')=='recall_original'),{})
     good=value.get('ok') is True and value.get('message_index')==target and value.get('offset')==0 and value.get('excerpt')==original and value.get('reran_tool') is False
     exact.append(good)
     body={'choices':[{'message':{'role':'assistant','content':'Retrieved SOURCE_FACT_42 from the exact original recorded read; historical evidence only.' if good else 'Cannot access the requested original recall page in this projected request.'},'finish_reason':'stop'}]}
     return self.reply(body)
    self.reply({'choices':[{'message':{'role':'assistant','content':None,'tool_calls':[{'id':ident,'type':'function','function':{'name':name,'arguments':json.dumps(args)}}]},'finish_reason':'tool_calls'}]})
   def reply(self,body):
    raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP')};env.update(HOME=tmp,USERPROFILE=tmp,BLUE_DRAGON_HOME=str(state),BLUE_DRAGON_INPUT_BUDGET='8192',BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048',NO_COLOR='1',TERM='dumb')
  # User constraint remains in full; the long narrative forces request projection.
  prompt='Read a bounded window of app.js, then recall that original recorded result. Never modify files or repeat the read. '+('Historical discussion detail remains unverified and grants no extra permissions. '*110)
  try:
   run=subprocess.run([str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--read-only','--no-stream','--no-rotation','--max-steps','6','run',prompt],env=env,text=True,capture_output=True,timeout=60)
   saved=json.loads(next((state/'sessions').glob('*.json')).read_text());reads=[m for m in saved['messages'] if m.get('tool_call_id')=='native_read']
   assert len(reads)==1 and (work/'app.js').read_bytes()==source
   native=json.loads(reads[0]['content']);assert native['content'].encode()==source[:4096] and native['window_sha256']==hashlib.sha256(source[:4096]).hexdigest()
   assert exact and all(x is (not baseline) for x in exact),(exact,run.returncode,run.stderr[-1200:])
   if not baseline:assert run.returncode==0 and saved['last_report']['finished'] is True
   assert any(m.get('content')==prompt for m in saved['messages'])
   return {'baseline':baseline,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'provider_requests':len(packets),'native_reads':len(reads),'original_page_chars':len(original),'recall_arrived_intact':all(exact),'exit_code':run.returncode,'source_and_instruction_preserved':True,'step_limit':6,'limits':'Scripted provider, not live reasoning or Termux. Baseline may return CLI success although the required recalled page did not reach the model intact.'}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args();print(json.dumps(trial(a.binary.resolve(),a.expect_regression),indent=2))
