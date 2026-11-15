#!/usr/bin/env python3
"""Real CLI/model/stdio resource path; synthetic loopback model and local MCP only."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


def main(binary):
    requests=[]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(packet)
            step=len(requests)
            if step==1:args={'action':'list','server':'fixture'}
            elif step in (2,5):args={'action':'read','server':'fixture','uri':'fixture://schema'}
            elif step==3:args={'action':'read','server':'fixture','uri':'fixture://unlisted'}
            else:args=None
            message={'role':'assistant','content':'Only bounded fixture context observed; no execution or security claim.'}
            if args:message['tool_calls']=[{'id':f'resource-{step}','type':'function','function':{'name':'mcp_resource_context','arguments':json.dumps(args)}}]
            body=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-mcp-resources-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state';log=root/'rpc.log';config=root/'servers.json'
            fixture=Path(__file__).resolve().parent.parent/'tests/fixtures/mcp_resource_server.py'
            config.write_text(json.dumps({'servers':[{'name':'fixture','command':sys.executable,'args':[str(fixture),'--log',str(log)]}]}))
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),NO_COLOR='1',TERM='dumb')
            command=[str(binary),'--root',str(workspace),'--mcp-config',str(config),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation']
            result=subprocess.run(command+['chat'],input='/connect fixture\ny\nRead approved fixture context.\ny\ny\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=30)
            assert result.returncode==0,result.stderr
            paths=list((app/'sessions').glob('*.json'));assert len(paths)==1
            saved=json.loads(paths[0].read_text());outputs=[json.loads(m['content']) for m in saved['messages'] if m['role']=='tool']
            assert outputs[0]['resources'][0]['uri']=='fixture://schema',outputs
            assert outputs[1]['contents'][0]['text']=='SCHEMA_FIXTURE',outputs
            assert outputs[1]['untrusted_external_content'] and not outputs[1]['security_claim_verified']
            assert outputs[2]['ok'] is False and len(requests)==4,outputs
            before=log.read_text();assert before.splitlines().count('resources/read')==1 and before.splitlines().count('initialize')==1,before
            resumed=subprocess.run(command+['--resume',saved['id'],'chat'],input='Inspect the previously mentioned URI without reconnecting.\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=30)
            assert resumed.returncode==0,resumed.stderr
            assert log.read_text()==before,'Resume must not reconnect or read the stale catalog'
            restored=json.loads(paths[0].read_text());latest=[json.loads(m['content']) for m in restored['messages'] if m['role']=='tool'][-1]
            assert latest['ok'] is False and len(requests)==6,latest
            print(json.dumps({'status':'passed','stdio_resource_reads':1,'unlisted_uri_requests':0,'resume_server_requests':0,'model_requests':6,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:server.shutdown();server.server_close();thread.join(timeout=3)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);main(p.parse_args().binary.resolve())
