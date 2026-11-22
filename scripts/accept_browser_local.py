#!/usr/bin/env python3
"""Real Chromium + Blue Dragon, fixed local page, simulated provider. No live targets."""
import argparse,hashlib,http.server,json,os,pathlib,select,subprocess,tempfile,threading,time
def fixture_env(home):
 env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
 env.update(HOME=str(home),BLUE_DRAGON_HOME=str(home/'state'),NO_COLOR='1',TERM='dumb')
 return env
p=argparse.ArgumentParser();p.add_argument('binary',type=pathlib.Path);p.add_argument('--chromium',default='/usr/bin/chromium');a=p.parse_args();binary=a.binary.resolve()
with tempfile.TemporaryDirectory(prefix='blue-browser-local-') as td:
 root=pathlib.Path(td);work=root/'work';work.mkdir();packets=[];gets=[];sequence=0
 script=(b'/* loopback test */\n'*14000)+b'const fixtureValue = 1;\n'
 assert len(script)>128_000

 class Local(http.server.BaseHTTPRequestHandler):
  def log_message(self,*_):pass
  def do_GET(self):
   gets.append(self.path)
   if self.path=='/fixture.js':
    self.send_response(200);self.send_header('Content-Type','application/javascript')
    self.send_header('Content-Length',str(len(script)));self.end_headers();self.wfile.write(script);return
   body=(b'<title>After</title><h1>Reached</h1>' if self.path=='/done' else b'<title>Before</title><h1>Start</h1><input id="name"><button id="preview" onclick="document.querySelector(&quot;output&quot;).textContent=document.querySelector(&quot;#name&quot;).value">Preview</button><output>Waiting</output><a id="next" href="/done">Next</a><script src="/fixture.js"></script>')
   self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(body)
  def do_POST(self):
   global sequence
   q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q)
   calls=[('snapshot',{'compact':True}),('click',{'selector':'#next'}),('wait_for',{'selector':'h1','text':'Reached'}),('snapshot',{'compact':True,'selector':'h1'})]
   m={'role':'assistant','content':'Observed final page: After, heading Reached. No further interaction.'}
   if sequence<len(calls):
    action,args=calls[sequence];args['action']=action
    advertised={t['function']['name'] for t in q.get('tools',[])}
    name='browser' if 'browser' in advertised else 'discover_tools'
    m={'role':'assistant','content':None,'tool_calls':[{'id':'b'+str(len(packets)),'type':'function','function':{'name':name,'arguments':json.dumps(args if name=='browser' else {'names':['browser']})}}]}
    if name=='browser':sequence+=1
   body=json.dumps({'choices':[{'message':m,'finish_reason':'tool_calls' if m.get('tool_calls') else 'stop'}]}).encode()
   self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Local);threading.Thread(target=server.serve_forever,daemon=True).start();site=f'http://127.0.0.1:{server.server_port}'
 browser=None;agent=None
 try:
  profile=root/'chrome';profile.mkdir()
  browser=subprocess.Popen([a.chromium,'--headless=new','--remote-debugging-address=127.0.0.1','--remote-debugging-port=0','--no-first-run','--no-default-browser-check','--user-data-dir='+str(profile),'about:blank'],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
  deadline=time.monotonic()+20
  while not (profile/'DevToolsActivePort').exists():
   if browser.poll() is not None:raise RuntimeError('Chromium startup failed; no sandbox workaround attempted: '+browser.stderr.read().decode()[-1500:])
   if time.monotonic()>deadline:raise RuntimeError('Chromium startup deadline')
   time.sleep(.05)
  port=(profile/'DevToolsActivePort').read_text().splitlines()[0]
  env=fixture_env(root)
  agent=subprocess.Popen([str(binary),'--root',str(work),'--base-url',site+'/v1','--model','fixture','--no-stream','--no-rotation','chat'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  def wait_prompt(approve=False):
   text=b'';answered=0;end=time.monotonic()+40
   while time.monotonic()<end:
    if select.select([agent.stdout],[],[],.05)[0]:
     chunk=os.read(agent.stdout.fileno(),65536)
     if not chunk:break
     text+=chunk
    n=text.count(b'Approve this ONE action?')+text.count(b'Allow snapshot and network reads for this page and task?')
    if n>answered:
     assert approve,'unexpected approval';agent.stdin.write((b'n\n' if approve=='deny' else b'y\n')*(n-answered));agent.stdin.flush();answered=n
    if b'blue-dragon> ' in text:return text.decode()
   raise AssertionError('CLI did not return to prompt: '+text.decode()[-3000:])
  wait_prompt()
  def send(text,approve=False):agent.stdin.write((text+'\n').encode());agent.stdin.flush();return wait_prompt(approve)
  send('/browser attach '+port)
  send('/scope allow '+site+'/ mutate')
  send('/browser do '+json.dumps({'action':'open','url':site+'/start'}),True)
  def browser_do(action, approve=False, **args):
   return send('/browser do '+json.dumps(dict(action=action,**args)),approve)
  def result(text):
   return json.JSONDecoder().raw_decode(text[text.index('{'):])[0]
  net=result(browser_do('network',True))
  same=result(browser_do('snapshot',compact=True))
  assert same['ok'] and same['result']['title']=='Before',same
  rows=net['result']['entries']
  row=next(r for r in rows if r['event']=='response' and r['url']==site+'/fixture.js')
  denied=result(browser_do('save_response','deny',request_id=row['request_id'],path='denied.js'))
  assert denied['denied'] is True and not (work/'denied.js').exists()
  saved=result(browser_do('save_response',True,request_id=row['request_id'],path='saved.js'))
  assert saved['bytes']==len(script) and saved['sha256']==hashlib.sha256(script).hexdigest()
  assert saved['complete_browser_body'] and (work/'saved.js').read_bytes()==script
  denied_overwrite=browser_do('save_response',request_id=row['request_id'],path='saved.js')
  assert 'Destination already exists' in denied_overwrite and (work/'saved.js').read_bytes()==script
  assert gets.count('/fixture.js')==1,gets
  field=result(browser_do('fill',True,selector='#name',value='Cedar-72'))
  assert field['ok'] and field['result']['matched'],field
  click=result(browser_do('click',True,selector='#preview'))
  assert click['ok'] and click['result']['clicked'],click
  shown=result(browser_do('wait_for',True,selector='output',text='Cedar-72'))
  assert shown['ok'],shown
  answer=send('Follow the Next link on the local page, read the resulting URL, title and main heading, then report them and stop.',True)
  returned=result(browser_do('navigate',True,url=site+'/start'))
  assert returned['ok'],returned
  home=result(browser_do('snapshot',True,compact=True))
  assert home['ok'] and home['result']['url']==site+'/start' and home['result']['h1']==['Start'],home
  agent.stdin.write(b'/exit\n');agent.stdin.flush();agent.wait(timeout=5)
  s=json.loads(next((root/'state/sessions').glob('*.json')).read_text())
  observations=[o for o in s['observations'] if o['tool']=='browser'];values=[json.loads(s['messages'][o['message_index']]['content']) for o in observations]
  calls=[c for m in s['messages'][s['user_turns'][-1]:] for c in m.get('tool_calls',[]) if c['function']['name']=='browser']
  assert sum(json.loads(c['function']['arguments'])['action']=='click' for c in calls)==1
  assert len(calls)==4,(calls,answer)
  last=values[-1];assert last['ok'] is True
  assert last['result']['url']==site+'/done' and last['result']['title']=='After' and last['result']['h1']==['Reached'],last
  assert last['result']['compact'] is True and len(last['result']['elements'])==1
  assert s['last_report']['finished'] is True and s['last_report']['task_status']!='needs_verification',s['last_report']
  print(json.dumps({'passed':True,'real_browser':'Chromium CDP','provider':'local scripted','browser_tool_calls':4,'clicks':1,'final_url_path':'/done','title':'After','h1':['Reached'],'recall_calls':0,'physical_phone':False,'preview_text':'Cedar-72','returned_heading':'Start','saved_browser_bytes':len(script),'asset_http_requests':gets.count('/fixture.js'),'denial_no_file':True,'overwrite_blocked':True}))
 finally:
  if agent and agent.poll() is None:agent.terminate();agent.wait(timeout=5)
  if browser and browser.poll() is None:browser.terminate();browser.wait(timeout=5)
  server.shutdown();server.server_close()
