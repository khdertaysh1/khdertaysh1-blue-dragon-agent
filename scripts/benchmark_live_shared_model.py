#!/usr/bin/env python3
"""Compare two agents on an exact synthetic read with one fixed live model."""
import argparse,http.server,json,os,pathlib,subprocess,tempfile,threading,time,urllib.request,urllib.error
from concurrent.futures import ThreadPoolExecutor
MODEL='nvidia/nemotron-3.5-lightning:free'
BASE='https://ai-gateway.kilo.ai/api/v1/chat/completions'
MARKER='BLUE_COMPARE_'+os.urandom(6).hex()
PROMPT='Use your local file reading tool to read sample.txt. Then return its exact entire text. Do not edit anything or run shell commands.'
def run(engine,binary):
    with tempfile.TemporaryDirectory(prefix='bd-fixed-model-') as d:
        root=pathlib.Path(d); work=root/'work'; work.mkdir()
        marker=MARKER
        file=work/'sample.txt'; file.write_text(marker)
        requests=[]
        class Proxy(http.server.BaseHTTPRequestHandler):
            def log_message(self,*args): pass
            def do_POST(self):
                req=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                if len(requests)>=5 or not self.path.endswith('/chat/completions') or req.get('model')!=MODEL:
                    self.send_error(422);return
                req['stream']=False;req.pop('stream_options',None)
                req.pop('max_completion_tokens',None);req['max_tokens']=1800
                item={'tools_advertised':len(req.get('tools',[])),'returned_model':None,'status':None}
                requests.append(item)
                outbound=urllib.request.Request(BASE,json.dumps(req).encode(),{'Content-Type':'application/json'},method='POST')
                try:
                    with urllib.request.urlopen(outbound,timeout=35) as res:
                        body=res.read(2000000); status=res.status
                except urllib.error.HTTPError as exc:
                    body=exc.read(100000);status=exc.code
                except Exception:
                    self.send_error(502);return
                value=json.loads(body);item.update(status=status,returned_model=value.get('model'))
                message=(value.get('choices') or [{}])[0].get('message',{})
                item['tool_names']=[c.get('function',{}).get('name') for c in message.get('tool_calls',[])]
                item['finish_reason']=(value.get('choices') or [{}])[0].get('finish_reason')
                item['marker_in_input']=any(marker in str(m.get('content')) for m in req.get('messages',[]) if m.get('role')=='tool')
                item['marker_in_assistant_reply']=marker in str(message.get('content') or '')
                # AlphaCode and Blue Dragon can request different transport modes.
                # Rewrap a single identical upstream completion as SSE for streamed clients.
                msg=dict(message); msg.pop('role',None)
                for i,call in enumerate(msg.get('tool_calls',[])): call['index']=i
                if self.headers.get('Accept','').find('text/event-stream')>=0 or engine=='alpha':
                    if status==200:
                        packet={'id':'compare','object':'chat.completion.chunk','model':value.get('model'),'choices':[{'index':0,'delta':msg,'finish_reason':None}]}
                        done={'id':'compare','object':'chat.completion.chunk','model':value.get('model'),'choices':[{'index':0,'delta':{},'finish_reason':item['finish_reason'] or 'stop'}]}
                        body=('data: '+json.dumps(packet)+'\n\ndata: '+json.dumps(done)+'\n\ndata: [DONE]\n\n').encode()
                        mime='text/event-stream'
                    else: mime='application/json'
                else: mime='application/json'
                self.send_response(status);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Proxy)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        url=f'http://127.0.0.1:{server.server_port}/v1'
        env={'HOME':d,'PATH':os.environ['PATH'],'TERM':'dumb','NO_COLOR':'1','BLUE_DRAGON_HOME':str(root/'blue'),'ALPHACODE_HOME':str(root/'alpha'),'XDG_CONFIG_HOME':str(root/'config'),'XDG_DATA_HOME':str(root/'data'),'ALPHACODE_OPENAI_COMPAT_API_BASE':url,'OPENAI_COMPAT_API_KEY':'local-proxy-placeholder','ALPHACODE_RUN_MCP':'0','ALPHACODE_RUN_AUTO_POKE':'0','ALPHACODE_NO_TELEMETRY':'1','ALPHACODE_OPENROUTER_CONTEXT_WINDOW':'65536'}
        args=([binary,'--root',str(work),'--base-url',url,'--model',MODEL,'--no-stream','--no-rotation','--max-steps','5','run',PROMPT] if engine=='blue' else
              [binary,'--provider','openai-compatible','--model',MODEL,'--no-update','--no-selfdev','-C',str(work),'run','--json',PROMPT])
        start=time.monotonic()
        try:
            proc=subprocess.run(args,input='',text=True,capture_output=True,timeout=90,env=env,cwd=work)
            output=proc.stdout
            passed=proc.returncode==0 and marker in output and requests and all(r['returned_model']==MODEL for r in requests) and any(r.get('marker_in_input') and r.get('marker_in_assistant_reply') for r in requests)
            return {'engine':engine,'passed':bool(passed),'returncode':proc.returncode,'requests':requests,'unchanged':file.read_text()==marker,'elapsed_seconds':round(time.monotonic()-start,2),'diagnostic_if_failed':None if passed else (proc.stderr+output)[-400:].replace(marker,'<fixture>')}
        except subprocess.TimeoutExpired:
            return {'engine':engine,'passed':False,'timeout':True,'requests':requests}
        finally: server.shutdown();server.server_close()
if __name__ == '__main__':
    parser=argparse.ArgumentParser(description='Opt-in comparison through Kilo using a synthetic local file only.')
    parser.add_argument('--blue', required=True, type=pathlib.Path)
    parser.add_argument('--alpha', required=True, type=pathlib.Path)
    parser.add_argument('--send-synthetic-to-kilo', action='store_true', required=True)
    args=parser.parse_args()
    ENGINES={'blue':str(args.blue.resolve()),'alpha':str(args.alpha.resolve())}
    with ThreadPoolExecutor() as e:
        print(json.dumps({'model':MODEL,'fixture':'read a synthetic file and return exact marker','rows':list(e.map(lambda p:run(*p),ENGINES.items()))},indent=2))
