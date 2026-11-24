#!/usr/bin/env python3
"""Real CLI fetch/search/scoped-flow/read over the pinned lab's actual JS source."""
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
from lab_matrix import LAB_COMMIT,loopback_only
from smoke_continuity import call

def main(binary,lab_path,mutation=None):
    assert subprocess.check_output(['git','-C',str(lab_path),'rev-parse','HEAD'],text=True).strip()==LAB_COMMIT
    subprocess.run(['git','-C',str(lab_path),'diff','--quiet','HEAD','--'],check=True)
    sys.dont_write_bytecode=True;sys.addaudithook(loopback_only);sys.path.insert(0,str(lab_path))
    from app import make_server
    from studio_browser import TARGET_JS
    source=TARGET_JS if mutation is None else f"function run(input){{let staged=input; {mutation} const alias=staged; node.innerHTML=alias;}}"
    search_query='const r=' if mutation is None else 'let staged'
    query='r' if mutation is None else 'input'
    lab=make_server(0);gets=[];original=lab.RequestHandlerClass
    class Counted(original):
        def do_GET(self):
            gets.append(self.path);assert gets==['/studio-target.js'];super().do_GET()
        def reply(self,status,data,mime='application/json; charset=utf-8',csp=None,headers=None):
            if mutation is not None and self.path=='/studio-target.js':data=source.encode()
            super().reply(status,data,mime,csp,headers)
    lab.RequestHandlerClass=Counted;thread=threading.Thread(target=lab.serve_forever,daemon=True);thread.start()
    url=f'http://127.0.0.1:{lab.server_port}/studio-target.js';packets=[]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(packet);step=len(packets)
            prior=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
            if step==1:args={'action':'fetch','url':url}
            elif step==2:args={'action':'search','artifact_id':prior[0]['artifact_id'],'query':search_query}
            elif step==3:args={'action':'flow','artifact_id':prior[0]['artifact_id'],'query':query,'offset':prior[1]['matches'][0]['offset']}
            elif step==4 and mutation is None:
                endpoint=next(s for s in prior[2]['steps'] if s['syntax']['target']=='out.textContent')
                args=endpoint['read_suggestion']
            else:args=None
            message=call('source_artifact',args,'flow-'+str(step)) if args else {'role':'assistant','content':'Source syntax inspected; HTTP response output path is not execution or security proof.'}
            data=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    model=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);mt=threading.Thread(target=model.serve_forever,daemon=True);mt.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-scoped-flow-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            cmd=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{model.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','6','chat']
            child=subprocess.run(cmd,env=env,cwd=workspace,input=f'/scope allow {url} read\nInspect the response-to-output source path only.\ny\n/exit\n',text=True,capture_output=True,timeout=30)
            assert child.returncode==0,child.stderr[-2000:]
            session=json.loads(next((app/'sessions').glob('*.json')).read_text())
            results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool'];assert len(results)==(4 if mutation is None else 3) and len(packets)==(5 if mutation is None else 4)
            fetched,search,flow=results[:3]
            assert fetched['artifact_id']==hashlib.sha256(source.encode()).hexdigest()
            assert search['matches'][0]['offset']==source.index(search_query)
            assert flow['ok'] is True
            assert flow['semantic_dataflow_verified'] is False and flow['network_requests']==0
            if mutation is None:
                read=results[3]
                assert flow['selected_function_scope']['kind']=='arrow_function'
                endpoint=next(s for s in flow['steps'] if s['syntax']['target']=='out.textContent')
                assert endpoint['from_symbol']=='r' and endpoint['semantic_flow_verified'] is False
                start,end=endpoint['syntax']['byte_offset'],endpoint['syntax']['end_byte']
                assert read['excerpt']==source.encode()[start:end].decode()
                assert 'r.status' in read['excerpt'] and 'r.json()' in read['excerpt']
            else:
                assert flow['limited'] is True
                alias=next(s for s in flow['steps'] if s['syntax']['target']=='alias')
                assert alias['possible_competing_writes'] is True and alias['followed_local_identifier'] is False
                assert not any(s['syntax']['target']=='node.innerHTML' for s in flow['steps'])
            assert gets==['/studio-target.js']
            print(json.dumps({'status':'passed','lab_commit':LAB_COMMIT,'GETs':1,'analysis_GETs':0,'fixture':'unmodified_lab_source' if mutation is None else 'synthetic_response_adapter','mutation':mutation,'selected_scope':flow['selected_function_scope'],'steps':len(flow['steps']),'limited':flow['limited'],'competing_write_stops_expansion':mutation is not None,'exact_source_oracle_matched':True,'semantic_dataflow_verified':False,'browser_execution_verified':False,'evidence_ids':[r['evidence_id'] for r in results],'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        lab.shutdown();lab.server_close();thread.join(timeout=3);model.shutdown();model.server_close();mt.join(timeout=3)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--lab',type=Path,required=True);a=p.parse_args()
    for mutation in (None,'staged += suffix;','staged++;'):main(a.binary.resolve(),a.lab.resolve(),mutation)
