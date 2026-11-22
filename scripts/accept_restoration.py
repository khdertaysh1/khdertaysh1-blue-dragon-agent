#!/usr/bin/env python3
"""Recovery acceptance: real CLI, local scripted provider, local files/HTTP only."""
import argparse,hashlib,http.server,json,os,pathlib,select,subprocess,tempfile,threading,time

def trial(binary,case,baseline=False):
 with tempfile.TemporaryDirectory(prefix='blue-restoration-') as td:
  root=pathlib.Path(td);work=root/'work';work.mkdir();home=root/'state';packets=[];gets=[];cursor=0;serial=0
  html='<html><title>Recovery Lab</title><h1>Local evidence</h1><script src="/module.js"></script></html>'
  class Lab(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_GET(self):
    gets.append(self.path);self.send_response(200);self.send_header('X-Recovery-Fixture','local-only');self.end_headers();self.wfile.write(html.encode())
  lab=http.server.ThreadingHTTPServer(('127.0.0.1',0),Lab);threading.Thread(target=lab.serve_forever,daemon=True).start();url=f'http://127.0.0.1:{lab.server_port}/'
  (work/'module.js').write_text('import { sum as add } from "./helper.js";\nexport { add as total };\nexport function compute(x) { return add(x, 1); }\n')
  (work/'helper.js').write_text('export function sum(a,b) { return a+b; }\n')
  command={'program':'printf','args':['BLUE_DRAGON_OK\n'],'timeout_seconds':5}
  read=lambda path:('read_file_window',{'path':path,'byte_offset':0,'byte_count':4096})
  bad=dict(command,cwd='synthetic-unused-field')
  sequences={
   'command':[('run_command',command)],'denied':[('run_command',command)],
   'file':[('write_file',{'path':'note.txt','content':'marker=BD_CHECK_942\nitems=7\nprice_each=6\n'}),read('note.txt')],
   'fetch':[('run_command',{'program':'curl','args':['-fsS','-D','headers.txt','-o','page.html',url],'timeout_seconds':10}),read('page.html'),read('headers.txt')],
   'search':[('search_file',{'path':'module.js','query':r'compute\s*\(','byte_offset':0,'scan_bytes':1024,'max_matches':5}),('search_file',{'path':'module.js','query':'compute(','byte_offset':0,'scan_bytes':1024,'max_matches':5}),read('module.js'),read('helper.js')],
   'schema':[('run_command',bad),('discover_tools',{'names':['run_command']}),('run_command',command),('run_command',dict(bad,cwd='second-synthetic-field')),('run_command',command)],
   'discovered':[('read_lines',{'path':'module.js','start':1,'count':3}),('discover_tools',{'names':['read_lines']}),('read_lines',{'path':'module.js','start':1,'count':3})],
   'repeat_bad':[('run_command',bad),('run_command',bad)],
   'discovery_only':[('run_command',bad),('discover_tools',{'names':['run_command']})],
   'fetch_discovery_recovery':[('web_fetch',{'url':url}),('discover_tools',{'names':['web_fetch']}),('web_fetch',{'url':url})],
   'fetch_discovery_partial':[('web_fetch',{'url':url}),read('module.js')],
   'fetch_discovery_denied':[('web_fetch',{'url':url}),('discover_tools',{'names':['web_fetch']}),('web_fetch',{'url':url})],

  }
  # Deliberately imperfect provider. The acceptance oracle examines the real file.
  exact_body = 'marker=BD_CHECK_942\nitems=7\nprice_each=6\n'
  exact_requests = {
   'exact_file': (exact_body, exact_body[:-1]),
   'exact_no_lf': ('سطر', 'سطر\n'),
   'exact_empty': ('', ''),
   'exact_denied': (exact_body, exact_body[:-1]),
   'exact_false_success': (exact_body, None),
  }
  if case in exact_requests:
   expected, proposed = exact_requests[case]
   if case == 'exact_false_success':
    (work/'note.txt').write_bytes(exact_body[:-1].encode())
   sequences[case] = [] if proposed is None else [('write_file', {'path':'note.txt', 'content':proposed})]
   if case not in ('exact_denied','exact_false_success'):
    sequences[case].append(read('note.txt'))
  seq=sequences[case]
  class Provider(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    nonlocal cursor,serial
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q);serial+=1
    defs={d['function']['name']:d['function']['parameters'] for d in q.get('tools',[])}
    if cursor<len(seq):
     name,args=seq[cursor]
     if case.startswith('fetch_discovery') and name=='web_fetch':
      assert (name in defs) == (cursor > 0), (cursor, sorted(defs))
      cursor+=1
     elif name not in defs and name!='run_command' and case!='discovered':name,args='discover_tools',{'names':[name]}
     else:cursor+=1
     m={'role':'assistant','content':None,'tool_calls':[{'id':f'c{serial}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
    else:
     results=[]
     for msg in q['messages']:
      if msg['role']=='tool':
       try:results.append(json.loads(msg['content']))
       except ValueError:pass
     # Report only actual returned content: an oracle checks file bytes and transcript separately.
     text='Original observations: '+json.dumps([{k:r[k] for k in ('stdout','exit_code','content','status','denied') if k in r} for r in results])
     for r in results:
      content=r.get('content','')
      if 'marker=BD_CHECK_942' in content:
       fields=dict(line.split('=',1) for line in content.splitlines() if '=' in line)
       text+=' Saved marker: '+fields['marker']+'; total='+str(int(fields['items'])*int(fields['price_each']))
     if case in ('exact_false_success', 'exact_denied'):
      text = 'SYNTHETIC_FALSE_CLAIM: file created with exact content.'
     m={'role':'assistant','content':text}
    body=json.dumps({'choices':[{'message':m,'finish_reason':'tool_calls' if m.get('tool_calls') else 'stop'}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  provider=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=provider.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')};env.update(HOME=td,BLUE_DRAGON_HOME=str(home),NO_COLOR='1',TERM='dumb',BLUE_DRAGON_INPUT_BUDGET='20000')
  prompt={'command':'Print BLUE_DRAGON_OK once locally, wait for my approval, then report stdout and exit code and stop.','denied':'Propose printing a local marker. Respect my decision; do not use an alternative.','file':'Create note.txt with marker=BD_CHECK_942, items=7, price_each=6 on three lines. Read it and report the marker and total price.','fetch':f'Using curl, save HTML from {url} to page.html and response headers to headers.txt. Read both files and report what they contain. Do not use a browser.','search':'Search module.js literally for the function name and read its complete definition.','schema':'Print a marker with my approval; correct invalid arguments without guessing and keep previous results.','repeat_bad':'Print a marker only with valid arguments and my approval.','discovery_only':'Print a marker only with valid arguments and my approval.','discovered':'Use the line-reading tool to inspect the first three lines of module.js.'}.get(case)
  if case.startswith('fetch_discovery'):
   prompt='Fetch the loopback test page only after approval. Continue with saved observations if the tool is unavailable.'
  if case in exact_requests:
   quoted = json.dumps(expected, ensure_ascii=False)
   prompt = r'Create note.txt with exactly this content, interpreting \n as newline characters: ' + quoted + '. Use native approval. Read the saved file with a file tool. Report its marker, byte count and ends_with_newline, then calculate items times price_each. Stop if denied. Do not repeat identical writes.'
  cmd=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{provider.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','12','run',prompt]
  try:
   if case=='exact_file':
    from pty_helpers import exact_file_chat
    stdout, code = exact_file_chat(cmd[:-2]+['chat'], prompt, env, work/'note.txt', expected)
   elif case=='command':
    proc=subprocess.Popen(cmd,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);out=b'';deadline=time.monotonic()+20
    while b'Approve this ONE action?' not in out and time.monotonic()<deadline:
     if select.select([proc.stdout],[],[],.1)[0]:out+=os.read(proc.stdout.fileno(),65536)
    assert b'Approve this ONE action?' in out,out[-2000:];before=len(packets);time.sleep(.2);assert len(packets)==before
    rest,_=proc.communicate(b'y\n',timeout=30);stdout=(out+rest).decode();code=proc.returncode
   else:
    proc=subprocess.run(cmd,input=('n\n' if case in ('denied','exact_denied','fetch_discovery_denied') else 'y\n'*10),env=env,text=True,capture_output=True,timeout=35);stdout=proc.stdout+proc.stderr;code=proc.returncode
   session=json.loads(next((home/'sessions').glob('*.json')).read_text());results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool'];commands=[r for r in results if 'exit_code' in r];errors=[r for r in results if r.get('category')=='invalid_arguments' and r.get('expected_type')!='advertised tool']
   advertised_first=any(d['function']['name']=='run_command' for d in packets[0].get('tools',[]))
   if baseline:return {'case':case,'first_run_command_advertised':advertised_first,'commands':len(commands),'argument_errors':len(errors),'task_status':session['last_report']['task_status'],'requests':len(packets),'exit':code}
   assert advertised_first
   for q in packets:
    for d in q.get('tools',[]):
     if d['function']['name']=='run_command':
      s=d['function']['parameters'];assert set(s['properties'])=={'program','args','timeout_seconds'} and s['additionalProperties'] is False
   if case in ('repeat_bad','discovery_only','exact_false_success','exact_denied','fetch_discovery_partial','fetch_discovery_denied'):assert code!=0 and not commands
   else:assert code==0,(case,stdout[-2000:])
   if case in exact_requests:
    note = work/'note.txt'
    if case in ('exact_denied', 'exact_false_success'):
     assert 'SYNTHETIC_FALSE_CLAIM' not in stdout
     if case=='exact_denied':
      assert not note.exists() and stdout.count('Approve this ONE action?')==1
     else:
      assert note.read_bytes()==exact_body[:-1].encode() and 'Expected 41 bytes' in stdout and 'observed 40 bytes' in stdout
    else:
     assert note.read_bytes()==expected.encode() and stdout.count('Approve this ONE action?')==1
     assert any(r.get('content')==expected and r.get('file_bytes')==len(expected.encode()) for r in results)
     assert f'Verified note.txt: {len(expected.encode())} bytes' in session['messages'][-1]['content']
     if case=='exact_file':
      assert 'total=42' in session['messages'][-1]['content']
      assert any(r.get('executed_arguments',{}).get('content')==expected for r in results)
   if case=='schema':assert len(errors)==2 and all(r['expected_type']=='only advertised fields' and r['actual_type']=='unexpected field' and r['correction_attempt']==1 for r in errors) and len(commands)==2
   if case=='command':assert len(commands)==1 and commands[0]['stdout']=='BLUE_DRAGON_OK\n' and commands[0]['exit_code']==0 and len(packets)==2
   if case=='denied':assert not commands and any(r.get('denied') or r.get('status')=='denied by user' for r in results) and len(packets)==2
   if case=='file':assert (work/'note.txt').read_text()=='marker=BD_CHECK_942\nitems=7\nprice_each=6\n' and any(r.get('content')==(work/'note.txt').read_text() for r in results)
   if case=='fetch':
    assert gets==['/'] and len(commands)==1 and commands[0]['exit_code']==0
    assert (work/'page.html').read_text()==html and 'X-Recovery-Fixture: local-only' in (work/'headers.txt').read_text()
    assert all(any(r.get('content')==p.read_text() for r in results) for p in [work/'page.html',work/'headers.txt'] if p.name=='page.html')
    assert any('X-Recovery-Fixture: local-only' in r.get('content','') for r in results)
   if case=='search':
    searches=[r for r in results if 'matches' in r];assert len(searches)==2 and searches[0]['matches']==[] and len(searches[1]['matches'])==1
   if case=='file':
    assert 'total=42' in session['messages'][-1]['content']
    before=len(packets);follow=subprocess.run(cmd[:-2]+['--resume',session['id'],'run','What was the marker and total from the previous observation? Do not use tools.'],input='',env=env,text=True,capture_output=True,timeout=30)
    assert follow.returncode==0 and len(packets)==before+1 and 'BD_CHECK_942' in follow.stdout and '42' in follow.stdout
   if case.startswith('fetch_discovery'):
    rejected=results[0]
    assert rejected['expected_type']=='advertised tool' and rejected['actual_type']=='unknown tool' and rejected['tool']=='web_fetch'
    assert rejected['execution_status']=='not_executed' and rejected['correction_attempt']==0
    assert '{"names":["web_fetch"]}' in rejected['retry_hint']
    assert 'web_fetch was not advertised' in stdout
    assert ('pending_tool_rejections' in json.dumps(packets[1])), packets[1].keys()
    if case=='fetch_discovery_recovery':
     assert gets==['/'] and any(r.get('status')==200 for r in results) and len(packets)==4
    elif case=='fetch_discovery_partial':
     assert gets==[] and any(r.get('content')==(work/'module.js').read_text() for r in results)
     assert 'Partial report' in stdout and 'Tool arguments remain unresolved: web_fetch was not advertised' in stdout
     assert len(packets)==3
    else:
     assert gets==[] and results[-1]['denied'] is True and len(packets)==4
   if case=='discovered':
    assert any(r.get('expected_type')=='advertised tool' and r['correction_attempt']==0 for r in results)
    assert any('lines' in r for r in results) and not errors
   if case in ('file','fetch'):assert session['last_report']['task_status']!='needs_verification',session['last_report']['task_status']
   return {'case':case,'passed':True,'requests':len(packets),'commands':len(commands),'GETs':len(gets),'argument_errors':len(errors),'task_status':session['last_report']['task_status']}
  finally:provider.shutdown();provider.server_close();lab.shutdown();lab.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('binary',type=pathlib.Path);p.add_argument('--case');p.add_argument('--baseline',action='store_true');a=p.parse_args();b=a.binary.resolve();cases=[a.case] if a.case else ['command','denied','file','fetch','search','schema','repeat_bad','discovery_only','discovered','exact_file','exact_no_lf','exact_empty','exact_denied','exact_false_success','fetch_discovery_recovery','fetch_discovery_partial','fetch_discovery_denied'];print(json.dumps({'sha256':hashlib.sha256(b.read_bytes()).hexdigest(),'local_scripted_provider':True,'rows':[trial(b,c,a.baseline) for c in cases]},indent=2))
