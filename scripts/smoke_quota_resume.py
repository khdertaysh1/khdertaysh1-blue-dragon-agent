#!/usr/bin/env python3
"""Bounded pre-stream 429 recovery; local provider, exact approval, no model rotation."""
import argparse,hashlib,http.server,json,os,subprocess,sys,tempfile,threading,time
from pathlib import Path

def trial(binary,mode,scope,delay):
 with tempfile.TemporaryDirectory(prefix='blue-quota-resume-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir();packets=[];times=[]
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))));times.append(time.monotonic());n=len(packets);header=None
    if mode=='action' and n==1:
     args={'program':sys.executable,'args':['-c',"from pathlib import Path;p=Path('count.txt');n=int(p.read_text()) if p.exists() else 0;p.write_text(str(n+1));print('once')"],'timeout_seconds':5}
     body={'choices':[{'message':{'role':'assistant','content':None,'tool_calls':[{'id':'once','type':'function','function':{'name':'run_command','arguments':json.dumps(args)}}]},'finish_reason':'tool_calls'}]};status=200
    elif n==(2 if mode=='action' else 1) or mode=='repeat':
     status=400 if mode=='400' else 429;body={'error':{'code':'tpm_rate_limit_exceeded','limit_source':scope,'message':'Synthetic quota rejection'}}
     if mode!='default':header=str(61 if mode=='long_block' else delay)
    else:status=200;body={'choices':[{'message':{'role':'assistant','content':'No additional action executed. Prior original results retained.'},'finish_reason':'stop'}]}
    raw=json.dumps(body).encode();self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)))
    if header is not None:self.send_header('Retry-After',header)
    self.end_headers();self.wfile.write(raw)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP')};env.update(HOME=tmp,USERPROFILE=tmp,BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
  try:
   prompt='Preserve scope. نفذ الأمر المحلي المحدد مرة واحدة فقط إذا طُلبت الموافقة. Keep original results; do not repeat actions.'
   command=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation']
   queued='FOLLOWUP: لا توسع النطاق ولا تعد أي إجراء مجهول. Preserve this new constraint.'
   command+=['chat'] if mode=='queued' else ['run',prompt]
   input_text=prompt+'\n'+queued+'\n/exit\n' if mode=='queued' else 'y\n' if mode=='action' else ''
   started=time.monotonic();p=subprocess.run(command,input=input_text,text=True,capture_output=True,env=env,timeout=delay+25)
   expected=mode in ('resume','action','queued');assert (p.returncode==0)==expected,(mode,p.stderr[-2000:])
   assert (len(packets) in (3,4) if mode=='action' else len(packets)==(2 if mode=='queued' else 2 if mode in ('resume','repeat') else 1)),(mode,len(packets))
   assert all(q['model']=='fixture' for q in packets)
   if len(packets)>1 and mode!='queued':
    a,b=(1,2) if mode=='action' else (0,1);assert times[b]-times[a]>=delay
    assert packets[a]['messages']==packets[b]['messages'] and packets[a].get('tools')==packets[b].get('tools')
   saved=json.loads(next((root/'state/sessions').glob('*.json')).read_text())
   if mode=='action':
    assert (work/'count.txt').read_text()=='1'
    results=[json.loads(m['content']) for m in saved['messages'] if m['role']=='tool'];assert len(results)==1 and results[0]['exit_code']==0 and results[0]['stdout'].strip()=='once'
   else:assert list(work.iterdir())==[]
   if mode=='queued':assert packets[1]['messages'][-1]['content']==queued and any(m.get('content')==queued for m in saved['messages'])
   assert saved['last_report']['finished']==expected
   return {'mode':mode,'scope':scope,'retry_after':delay if mode not in ('default','long_block') else None,'requests':len(packets),'seconds':round(time.monotonic()-started,3),'completion_followups':len(packets)-3 if mode=='action' else 0,'command_executions':1 if mode=='action' else 0,'model_unchanged':True,'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--long',action='store_true');a=p.parse_args();cases=[('resume','account',60)] if a.long else [('resume',s,0) for s in ('account','unknown','model')]+[(m,'account',0) for m in ('action','queued','repeat','400','default','long_block')]
 print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[trial(a.binary.resolve(),*case) for case in cases],'limits':'Scripted HTTP provider, Linux local command fixture only. No live capacity guarantee, model rotation or unknown-action retry. Long headers/default 300s remain explicit blockers.'},indent=2))
