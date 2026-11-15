#!/usr/bin/env python3
"""Actual CLI source refresh against a loopback version fixture and scripted model."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
from smoke_continuity import call


def main(binary):
    packets, gets = [], []
    old = 'export const version=1;'
    new = 'export const version=2;'
    class Server(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_GET(self):
            gets.append(self.path)
            payload = (old if len(gets) == 1 else new).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/javascript')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        def do_POST(self):
            packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            packets.append(packet)
            step = len(packets)
            if step <= 4:
                action = ['fetch', 'refresh', 'fetch', 'refresh'][step - 1]
                message = call('source_artifact', {'action':action, 'url':base+'/asset.js'}, str(step))
            elif step in (5,6,7):
                prior=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
                if step==5:
                    arguments={'action':'compare','baseline_artifact_id':prior[0]['artifact_id'],'artifact_id':prior[1]['artifact_id']}
                elif step==6:
                    arguments={'action':'read','artifact_id':prior[0]['artifact_id'],'count':100}
                else:
                    arguments=prior[4]['variant']['read_suggestion']
                message=call('source_artifact',arguments,'inspect-'+str(step))
            else:
                message = {'role':'assistant','content':'Both source versions retained; content difference is not a vulnerability finding.'}
            payload = json.dumps({'choices':[{'message':message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type','application/json')
            self.send_header('Content-Length',str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
    server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Server)
    base = f'http://127.0.0.1:{server.server_port}'
    thread = threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-source-refresh-') as temp:
            root = Path(temp)
            workspace = root/'workspace'
            workspace.mkdir()
            app = root/'state'
            env = {k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            command = [str(binary),'--root',str(workspace),'--base-url',base+'/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','8','chat']
            child = subprocess.run(command,env=env,cwd=workspace,text=True,capture_output=True,
                input=f'/scope allow {base}/ read\nRefresh fixture source explicitly and retain old version.\ny\ny\ny\n/exit\n',timeout=30)
            assert child.returncode==0,child.stderr[-3000:]
            files=list((app/'sessions').glob('*.json'))
            assert len(files)==1,files
            session=json.loads(files[0].read_text())
            results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool']
            assert len(results)==7,results
            first,changed,reused,unchanged,comparison,read,changed_read=results
            assert changed['previous_artifact_id']==first['artifact_id']
            assert changed['content_changed'] is True
            assert reused['reused_local_copy'] is True and reused['fresh_evidence'] is False
            assert unchanged['content_changed'] is False and unchanged['fresh_evidence'] is True
            assert read['excerpt']==old
            assert comparison['content_equal'] is False and comparison['semantic_change_verified'] is False
            assert comparison['network_requests']==0 and comparison['fresh_evidence'] is False
            assert changed_read['excerpt']=='2',changed_read
            assert len(gets)==3 and all(path=='/asset.js' for path in gets),gets
            index=json.loads((app/'source-artifacts'/session['id']/'index.json').read_text())
            assert len(index)==2,index
            assert all(len(json.dumps(r))<1600 for r in results)
            assert len(packets)==8,len(packets)
            print(json.dumps({'status':'passed','get_requests':3,'stored_versions':2,'old_version_readable':True,'unchanged_version_duplicated':False,'model_requests':8,'changed_span_readable':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    main(parser.parse_args().binary.resolve())
