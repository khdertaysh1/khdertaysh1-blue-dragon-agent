#!/usr/bin/env python3
"""Local scripted schema transition regression; no valid network target or browser action."""
import json,http.server,threading,subprocess,tempfile,os,pathlib,sys
binary=sys.argv[1]
baseline="--expect-regression" in sys.argv[2:]
for name,args in [('web_fetch',{'url':'bad-url'}),('browser',{'action':'status'})]:
 with tempfile.TemporaryDirectory() as d:
  p=pathlib.Path(d);(p/'work').mkdir();packets=[]
  class H(http.server.BaseHTTPRequestHandler):
   def log_message(self,*a):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(q);n=len(packets)
    if n<=3:
     tool,a=('discover_tools',{'names':[name]}) if n==2 else (name,args)
     msg={'role':'assistant','content':None,'tool_calls':[{'id':f'c{n}','type':'function','function':{'name':tool,'arguments':json.dumps(a)}}]};stop='tool_calls'
    else:msg={'role':'assistant','content':'Local test complete; no page was inspected.'};stop='stop'
    b=json.dumps({'choices':[{'message':msg,'finish_reason':stop}]}).encode();self.send_response(200);self.end_headers();self.wfile.write(b)
  s=http.server.ThreadingHTTPServer(('127.0.0.1',0),H);threading.Thread(target=s.serve_forever,daemon=True).start()
  env=dict(PATH=os.environ['PATH'],HOME=d,BLUE_DRAGON_HOME=str(p/'state'),BLUE_DRAGON_INPUT_BUDGET='8192',NO_COLOR='1')
  r=subprocess.run([binary,'--root',str(p/'work'),'--base-url',f'http://127.0.0.1:{s.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','6','run','Read-only local test; do not access any external site.'],env=env,capture_output=True,text=True,timeout=30)
  x=json.load(open(next((p/'state/sessions').glob('*.json'))));out=[json.loads(m['content']) for m in x['messages'] if m['role']=='tool']
  assert name not in [v['function']['name'] for v in packets[0]['tools']]
  assert name in [v['function']['name'] for v in packets[2]['tools']]
  assert out[0]['category']=='invalid_arguments' and out[0]['execution_status']=='not_executed'
  assert out[1]['ok'] is True and out[1]['permission_changed'] is False
  if baseline: assert out[2]['repetition_blocked'] is True and r.returncode==1
  else:
   assert out[2].get('repetition_blocked') is not True
   if name=='browser':
    assert out[2]['ready'] is False and out[2]['page_inspected'] is False and r.returncode==0
   else:
    # Invalid URL reaches native URL/scope validation; no network target exists.
    assert out[2]['ok'] is False and 'category' not in out[2] and r.returncode==1
  print(json.dumps({'tool':name,'baseline':baseline,'requests':len(packets),'exit':r.returncode,'advertisement_transition_verified':True,'post_discovery_blocked':out[2].get('repetition_blocked',False),'native_status_ready':out[2].get('ready'),'page_inspected':out[2].get('page_inspected'),'post_discovery_error':out[2].get('error'),'limits':'Programmed provider; no external target or real browser.'}));s.shutdown()
