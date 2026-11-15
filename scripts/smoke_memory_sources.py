#!/usr/bin/env python3
"""Actual CLI memory capture/restart/tamper/delete with no tool replay."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def main(binary):
    packets=[]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(packet);step=len(packets)
            results=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
            if step==1:name,args='read_file',{'path':'fixture.txt'}
            elif step==2:name,args='project_memory',{'action':'save','text':'fixture remembered source','kind':'lesson','evidence_ids':[results[0]['evidence_id']]}
            elif step==3:name,args='project_memory',{'action':'search','query':'fixture','limit':5}
            else:name,args=None,None
            message={'role':'assistant','content':'Historical note recorded; source matching is not current verification.'}
            if name:message['tool_calls']=[{'id':f'memory-{step}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]
            body=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-memory-source-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state';(workspace/'fixture.txt').write_text('PRIVATE_CAPTURE_SENTINEL')
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),NO_COLOR='1',TERM='dumb')
            cmd=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','chat']
            first=subprocess.run(cmd,input='Save and inspect a fixture memory note.\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=30);assert first.returncode==0,first.stderr
            source_path=next((app/'sessions').glob('*.json'));source=json.loads(source_path.read_text());results=[json.loads(m['content']) for m in source['messages'] if m['role']=='tool'];report=next(r for r in results if 'notes' in r)
            assert report['notes'][0]['source_validation']['all_references_match_historical_captures'],report
            assert not report['fresh_evidence'] and len(packets)==4
            def inspect(expected):
                result=subprocess.run(cmd,input='/memory fixture\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15)
                assert result.returncode==0,result.stderr
                assert expected in result.stdout,result.stdout
                assert 'PRIVATE_CAPTURE_SENTINEL' not in result.stdout
                assert len(packets)==4,'Memory inspection contacted the model'
            inspect('matched_historical_capture')
            index=source['observations'][0]['message_index'];source['messages'][index]['content']='{"content":"TAMPERED"}';source_path.write_text(json.dumps(source))
            inspect('changed_or_invalid_reference')
            source_path.unlink();inspect('session_unavailable_or_over_limit')
            assert (workspace/'fixture.txt').read_text()=='PRIVATE_CAPTURE_SENTINEL'
            print(json.dumps({'status':'passed','model_requests':4,'memory_inspection_model_requests':0,'tamper_detected':True,'missing_source_detected':True,'freshness_claimed':False,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:server.shutdown();server.server_close();thread.join(timeout=3)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);main(p.parse_args().binary.resolve())
