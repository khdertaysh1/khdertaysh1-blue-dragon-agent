#!/usr/bin/env python3
"""Actual CLI dynamic-source discovery on synthetic responses in the pinned local lab."""
import argparse,hashlib,http.server,json,os,subprocess,sys,tempfile,threading
from pathlib import Path
from lab_matrix import LAB_COMMIT,loopback_only
from smoke_continuity import call
from smoke_source_discovery import audit_from_stdout

def run(binary,make_server,mode):
    source="import('./lazy.js'); import(name); import(`./${name}.js`); import('./lazy.js'); obj.import('./fake.js');"
    sources={'/dynamic/app.js':source,'/dynamic/lazy.js':'export const lazy=true;'}
    lab=make_server(0);gets=[];original=lab.RequestHandlerClass
    class Adapter(original):
        def do_GET(self):
            assert self.valid_host() and self.path in sources
            gets.append(self.path);assert len(gets)<=2
            self.reply(200,sources[self.path].encode(),'text/javascript; charset=utf-8')
    lab.RequestHandlerClass=Adapter;worker=threading.Thread(target=lab.serve_forever,daemon=True);worker.start()
    base=f'http://127.0.0.1:{lab.server_port}';packets=[]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(packet)
            step=len(packets);prior=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
            if step==1:args={'action':'fetch','url':base+'/dynamic/app.js'}
            elif step in (2,5):args={'action':'discover','artifact_id':prior[0]['artifact_id']}
            elif step in (3,4):args={'action':'fetch_reference','artifact_id':prior[0]['artifact_id'],'reference_index':0 if step==3 else 1}
            else:args=None
            message=call('source_artifact',args,'dynamic-'+str(step)) if args else {'role':'assistant','content':'Recorded syntax only; computed dependencies remain unresolved.'}
            body=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    model=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);mt=threading.Thread(target=model.serve_forever,daemon=True);mt.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-dynamic-') as tmp:
            root=Path(tmp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            cmd=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{model.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','8','chat']
            scope=base+('/dynamic/app.js' if mode=='outside_scope' else '/dynamic/')
            answer='' if mode=='outside_scope' else ('n\n' if mode=='approval_denied' else 'y\n')
            child=subprocess.run(cmd,env=env,cwd=workspace,input=f'/scope allow {scope} read\nDiscover local module references.\ny\n'+answer+'/audit-report\n/exit\n',text=True,capture_output=True,timeout=30)
            assert child.returncode==0,child.stderr[-1000:]
            session=json.loads(next((app/'sessions').glob('*.json')).read_text());results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool']
            assert len(results)==5,results
            assert results[0]['artifact_id']==hashlib.sha256(source.encode()).hexdigest()
            first,last=results[1],results[4]
            assert len(first['references'])==4 and first['limited'] is True
            assert first['references'][0]['kind']=='literal_dynamic_import_candidate'
            assert first['references'][0]['source_byte_offset']==source.index('import(')
            assert [r['kind'] for r in first['references'][1:3]]==['unresolved_dynamic_import']*2
            assert all(r['target_redacted'] is None and not r['fetch_available'] for r in first['references'][1:3])
            assert first['references'][3]['state']=='duplicate_reference'
            assert first['counts']['unresolved']==2 and results[3]['ok'] is False
            if mode=='allowed':
                assert gets==list(sources)
                assert results[2]['artifact_id']==hashlib.sha256(sources['/dynamic/lazy.js'].encode()).hexdigest()
                assert last['counts']['unique_index_matches']==1
            else:
                assert gets==['/dynamic/app.js']
                assert results[2].get('denied') is True if mode=='approval_denied' else results[2]['ok'] is False
                assert last['counts']['unique_index_matches']==0
            report=audit_from_stdout(child.stdout);assert len(report['roots'])==1
            assert report['roots'][0]['collector_limited'] is True
            assert report['roots'][0]['counts_recomputed_from_recorded_references']['unresolved']==2
            assert report['network_requests']==0 and report['actions_replayed']==0
            return {'case':mode,'status':'passed','GETs':len(gets),'discovery_GETs':0,'unresolved':2,'collector_limited':True,'source_hashes_verified':True,'report_incomplete_verified':True,'browser_loading_verified':False}
    finally:
        lab.shutdown();lab.server_close();worker.join(timeout=3);model.shutdown();model.server_close();mt.join(timeout=3)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--lab',type=Path,required=True);a=p.parse_args();lab=a.lab.resolve();binary=a.binary.resolve()
    assert subprocess.check_output(['git','-C',str(lab),'rev-parse','HEAD'],text=True).strip()==LAB_COMMIT
    subprocess.run(['git','-C',str(lab),'diff','--quiet','HEAD','--'],check=True)
    sys.dont_write_bytecode=True;sys.addaudithook(loopback_only);sys.path.insert(0,str(lab));from app import make_server
    rows=[run(binary,make_server,m) for m in ('allowed','outside_scope','approval_denied')]
    print(json.dumps({'status':'passed','lab_commit':LAB_COMMIT,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rows':rows,'GETs':sum(r['GETs'] for r in rows),'limits':'Synthetic module adapters; source bytes and server counts are independent fixture oracles, not browser execution. Python audit guard does not OS-sandbox Rust.'}))
