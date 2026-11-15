#!/usr/bin/env python3
"""Actual CLI cached alias-chain traversal, followed by an exact source-range read."""
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


def main(binary, truncated=False):
    packets, gets = [], []
    sources={'/app.js':"import {sanitize as clean} from './bridge.js'; const output=clean(input);",
             '/bridge.js':"export {decode as sanitize} from './parse.js';",
             '/parse.js':"export function decode(value) { return value.trim(); }"}
    if truncated:
        sources['/app.js']="import {clean,"+','.join('name'+str(i) for i in range(40))+"} from './bridge.js'; clean(input);"
    class Server(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_GET(self):
            gets.append(self.path)
            assert self.path in sources
            payload = sources[self.path].encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/javascript')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        def do_POST(self):
            packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            packets.append(packet)
            step = len(packets)
            if step <= 3:
                message = call('source_artifact', {'action':'fetch','url':base+list(sources)[step-1]}, str(step))
            elif step == 4 or step == 5 and not truncated:
                prior=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
                if step==4:
                    arguments={'action':'trace_chain','artifact_id':prior[0]['artifact_id'],'query':'clean'}
                else:
                    node=prior[3]['nodes'][-1]
                    candidate=node['candidates'][0]
                    arguments={'action':'read','artifact_id':node['artifact_id'],'offset':candidate['byte_offset'],'count':candidate['end_byte']-candidate['byte_offset']}
                message=call('source_artifact',arguments,'inspect-'+str(step))
            else:
                message = {'role':'assistant','content':'Cached syntactic alias path only; no dataflow or sanitizer proof.'}
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
            if truncated:
                assert len(results)==4 and len(packets)==5
                chain=results[3]
                assert chain['limited'] is True and len(chain['nodes'])==1 and chain['edges']==[]
                assert chain['nodes'][0]['import_bindings_limited'] is True
                assert any(b['reason']=='import_binding_collection_incomplete' and b['bindings_omitted_count'] is None for b in chain['blocked'])
            else:
                assert len(results)==5,results
                chain,read=results[3:]
                assert chain['ok'] is True and len(chain['nodes'])==3 and len(chain['edges'])==2
                assert [n['query'] for n in chain['nodes']]==['clean','sanitize','decode']
                assert chain['semantic_dataflow_verified'] is False and chain['limited'] is False
                assert read['excerpt']=='function decode(value) { return value.trim(); }',read
                assert len(packets)==6
            assert chain['network_requests']==0 and chain['fresh_evidence'] is False
            assert gets==list(sources),gets
            assert len(json.dumps(chain))<10000
            print(json.dumps({'status':'passed','case':'omitted_import_binding' if truncated else 'complete_alias_chain','GETs':len(gets),'trace_GETs':0,'nodes':len(chain['nodes']),'edges':len(chain['edges']),'limited':chain['limited'],'blocked':chain['blocked'],'exact_follow_up_read':not truncated,'bytes_scanned':chain['bytes_scanned'],'semantic_dataflow_verified':False,'tool_evidence_ids':[r['evidence_id'] for r in results],'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest()}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    binary=parser.parse_args().binary.resolve()
    main(binary)
    main(binary,truncated=True)
