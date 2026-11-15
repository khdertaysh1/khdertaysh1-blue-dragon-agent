#!/usr/bin/env python3
"""Deterministic real CLI comparison; synthetic TPM model, not service capacity."""
import argparse,hashlib,http.server,json,os,subprocess,tempfile,threading,time
from pathlib import Path

def estimate(text):
    ascii_count=sum(ord(c)<128 for c in text)
    return (ascii_count+2)//3+(len(text)-ascii_count)*2

def run(binary,after,cap):
    packets=[];used=0;phase=0;useful=0
    class Server(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_POST(self):
            nonlocal used,phase,useful
            packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));tokens=sum(estimate(json.dumps(m,separators=(',',':'),ensure_ascii=False)) for m in packet['messages'])+estimate(json.dumps(packet.get('tools',[]),separators=(',',':'),ensure_ascii=False))
            row={'estimated_input_tokens':tokens,'schemas_bytes':len(json.dumps(packet.get('tools',[]))),'history_bytes':len(json.dumps(packet['messages'])),'max_output_tokens':packet.get('max_tokens'),'status':200};packets.append(row)
            assert any('NEVER MODIFY FILES' in str(m.get('content','')) for m in packet['messages'])
            if used+tokens>cap:
                row['status']=429;code=429;body={'error':{'code':'tpm_rate_limit_exceeded','message':'Synthetic rolling-minute input-token capacity exceeded'}}
            else:
                used+=tokens;code=200
                if phase%2==0:
                    msg={'role':'assistant','content':'Inspect current fixture version.','tool_calls':[{'id':f'new-{phase}','type':'function','function':{'name':'read_file_window','arguments':json.dumps({'path':'app.js','byte_offset':0,'byte_count':512})}}]};finish='tool_calls'
                else:
                    results=[str(m.get('content','')) for m in packet['messages'][-4:]];assert any('SAFE_SENTINEL_42' in r for r in results), 'Current bounded evidence absent from projection'
                    durable=json.loads(path.read_text());native=[m['content'] for m in durable['messages'] if m['role']=='tool'];assert native;observed=json.loads(native[-1]);expected=(workspace/'app.js').read_bytes()[:512];assert observed.get('ok') is not False and not observed.get('denied') and observed.get('execution_status')!='unknown' and observed.get('content')==expected.decode() and observed.get('window_sha256')==hashlib.sha256(expected).hexdigest()
                    useful+=1;msg={'role':'assistant','content':'The current bounded fixture contains SAFE_SENTINEL_42. This is an observation, not proof of security.'};finish='stop'
                phase+=1;body={'model':'deterministic-fixture','choices':[{'message':msg,'finish_reason':finish}],'usage':{'prompt_tokens':tokens,'completion_tokens':40,'total_tokens':tokens+40}}
            data=json.dumps(body).encode();self.send_response(code);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.send_header('x-ratelimit-limit-tokens',str(cap));self.end_headers();self.wfile.write(data)
    with tempfile.TemporaryDirectory(prefix='blue-capacity-') as folder:
        root=Path(folder);workspace=root/'project';workspace.mkdir();(workspace/'app.js').write_text('export const marker = "SAFE_SENTINEL_42";\n'+'// historical fixture padding\n'*4000)
        original=hashlib.sha256((workspace/'app.js').read_bytes()).hexdigest();app=root/'state'
        env={k:v for k,v in os.environ.items() if k in ['PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT']};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),NO_COLOR='1',TERM='dumb')
        if after:env.update(BLUE_DRAGON_INPUT_BUDGET='8192',BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048',BLUE_DRAGON_PROVIDER_AUDIT_DIR=str(root/'audit'))
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Server);threading.Thread(target=server.serve_forever,daemon=True).start()
        args=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--read-only','--max-steps','6']
        try:
            seed=subprocess.run(args+['chat'],input='/exit\n',text=True,capture_output=True,env=env,timeout=20);assert seed.returncode==0
            path=next((app/'sessions').glob('*.json'));session=json.loads(path.read_text());session['messages']=[{'role':'system','content':'Inspect only the requested local fixture; use native read evidence.'},{'role':'user','content':'NEVER MODIFY FILES. Preserve this constraint and original evidence.'}]
            for i in range(8):
                session['messages'] += [{'role':'assistant','content':None,'tool_calls':[{'id':f'old-{i}','type':'function','function':{'name':'read_file_window','arguments':'{"path":"app.js","byte_offset":0,"byte_count":6000}'}}]},{'role':'tool','tool_call_id':f'old-{i}','content':json.dumps({'ok':True,'content':'historical source '+('x'*6000),'source_version':i})}]
            history=len(session['messages']);path.write_text(json.dumps(session));start=time.monotonic()
            result=subprocess.run(args+['--resume',session['id'],'chat'],input='Inspect app.js and report the sentinel.\nDiscuss the evidence and read its current version.\nRecheck the current bounded evidence once more.\n/exit\n',text=True,capture_output=True,env=env,timeout=60)
            saved=json.loads(path.read_text());assert len(saved['messages'])>=history;assert hashlib.sha256((workspace/'app.js').read_bytes()).hexdigest()==original
            if after: assert useful==3 and len(packets)==6 and not any(p['status']==429 for p in packets), (useful,packets,result.stderr[-1500:],result.stdout[-2000:],saved.get("last_report",{}))
            return {'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'mode':'after' if after else 'before','synthetic_input_tpm':cap,'requests':len(packets),'http_429':sum(p['status']==429 for p in packets),'useful_verified_rounds':useful,'estimated_input_tokens_sent':sum(p['estimated_input_tokens'] for p in packets),'elapsed_seconds':round(time.monotonic()-start,3),'unchanged_file':True,'original_history_preserved':True,'packets':packets,'limits':'Scripted model decisions and synthetic input TPM accounting; not live model reasoning, billing tokens, or real-service capacity.'}
        finally:server.shutdown();server.server_close()
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--before',type=Path,required=True);p.add_argument('--after',type=Path,required=True);p.add_argument('--cap',type=int,default=60000);a=p.parse_args()
    print(json.dumps({'before':run(a.before.resolve(),False,a.cap),'after':run(a.after.resolve(),True,a.cap)},indent=2))
