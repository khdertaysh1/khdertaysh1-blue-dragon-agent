#!/usr/bin/env python3
"""Real CLI plan/fetch/evidence/report roundtrip using loopback-only fixtures."""
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
    packets, gets = [], []
    class Fixture(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_GET(self):
            gets.append(self.path)
            body = b'PRIVATE_RESPONSE_SENTINEL' if len(gets)==1 else b'PRIVATE_RETEST_SENTINEL'
            self.send_response(200)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def do_POST(self):
            packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            packets.append(packet)
            results = [json.loads(m['content']) for m in packet['messages'] if m['role'] == 'tool']
            step = len(packets)
            if step == 1:
                name, args = 'test_coverage', dict(action='plan', url=url, method='GET', account_label='fixture', scenario='http_response')
            elif step in (2,9):
                name, args = 'web_fetch', dict(url=url)
            elif step in (3,10):
                case = next(r['case_id'] for r in results if 'case_id' in r)
                fetch = next(r for r in reversed(results) if 'test_coverage_context' in r)
                name, args = 'test_coverage', dict(action='record', case_id=case, evidence_id=fetch['evidence_id'])
            elif step in (4,5):
                fetch=next(r for r in results if 'test_coverage_context' in r)
                name,args='record_finding',dict(status='hypothesis',summary='PRIVATE_RIVAL_'+str(step),evidence_ids=[fetch['evidence_id']],limitation='Loopback fixture only',rationale='Alternative interpretation',next_test='Repeat the same request',confirms='Response changed' if step==4 else 'Response stayed identical',refutes='Opposite response pattern')
            elif step==6:
                case=next(r['case_id'] for r in results if 'case_id' in r)
                name,args='test_coverage',dict(action='group_hypotheses',group_id='response-rivals',finding_indices=[0,1],case_ids=[case])
            elif step in (7,11):
                name,args='test_coverage',dict(action='report')
            else:
                name = None
            message = {'role':'assistant', 'content':'Fixture complete.'}
            if name:
                message['tool_calls'] = [{'id':f'call-{step}', 'type':'function', 'function':{'name':name, 'arguments':json.dumps(args)}}]
            body = json.dumps({'choices':[{'message':message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type','application/json')
            self.send_header('Content-Length',str(len(body)))
            self.end_headers()
            self.wfile.write(body)
    server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture)
    thread = threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    url = f'http://127.0.0.1:{server.server_port}/source'
    try:
        with tempfile.TemporaryDirectory(prefix='blue-test-coverage-') as temp:
            root=Path(temp); workspace=root/'workspace'; workspace.mkdir(); app=root/'state'
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            command=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','chat']
            result=subprocess.run(command,input=f'/scope allow {url} read fixture\nRun the local fixture.\ny\nRepeat the approved local response capture.\ny\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=30)
            assert result.returncode==0,result.stderr[-3000:]
            paths=list((app/'sessions').glob('*.json')); assert len(paths)==1
            saved=json.loads(paths[0].read_text())
            results=[json.loads(m['content']) for m in saved['messages'] if m['role']=='tool']
            report=next(r for r in reversed(results) if 'cases' in r)
            assert report['cases'][0]['state']=='tested',report
            assert report['cases'][0]['authentication_verified'] is False
            assert 'PRIVATE_RESPONSE_SENTINEL' not in json.dumps(report)
            assert gets==['/source','/source'],gets
            delta=report['cases'][0]['retest_deltas'][0]
            assert delta['comparable'] and delta['decoded_text_changed'] and not delta['status_changed'],delta
            group=report['hypothesis_groups'][0]
            assert group['winner'] is None and len(group['alternatives'])==2,group
            assert all(a['finding_unchanged'] and a['semantic_outcome']=='unresolved' for a in group['alternatives'])
            assert 'PRIVATE_' not in json.dumps(report)
            assert len(packets)==12,len(packets)
            # Resume and export after revocation without contacting the provider.
            resume=command[:-1]+['--resume',saved['id'],'chat']
            exported=subprocess.run(resume,input='/scope clear\n/coverage\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=30)
            assert exported.returncode==0,exported.stderr
            assert 'tested' in exported.stdout and 'current_scope_and_label_match' in exported.stdout
            assert 'PRIVATE_RESPONSE_SENTINEL' not in exported.stdout
            assert gets==['/source','/source'] and len(packets)==12
            print(json.dumps({'status':'passed','native_GETs':2,'model_requests':12,'competing_hypotheses':2,'retest_text_change_detected':True,'automatic_promotion':False,'resume_export_requests':0,'account_verified':False,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        server.shutdown();server.server_close();thread.join(timeout=3)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('binary',type=Path)
    main(parser.parse_args().binary.resolve())
