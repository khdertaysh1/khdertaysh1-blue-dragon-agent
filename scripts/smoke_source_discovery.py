#!/usr/bin/env python3
"""Actual CLI discovery -> approved selected fetch -> refreshed coverage on pinned lab."""
import argparse
import hashlib
from html.parser import HTMLParser
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
from lab_matrix import LAB_COMMIT, loopback_only
from fixture_intent import reply_to_work_intent
from smoke_continuity import call

def audit_from_stdout(text):
    decoder=json.JSONDecoder()
    for at,char in enumerate(text):
        if char!='{':continue
        try:value,_=decoder.raw_decode(text[at:])
        except json.JSONDecodeError:continue
        if isinstance(value,dict) and value.get('schema')==2 and 'source_discovery' in value:
            return value['source_discovery']
    raise AssertionError('CLI audit report missing')

class ScriptOracle(HTMLParser):
    def __init__(self):
        super().__init__();self.sources=[]
    def handle_starttag(self,tag,attrs):
        if tag=='script':
            src=dict(attrs).get('src')
            if src:self.sources.append(src)

def run(binary,make_server,expected,scoped,malformed=False):
    lab=make_server(0);gets=[];original=lab.RequestHandlerClass
    class Counted(original):
        def do_GET(self):
            gets.append(self.path);assert len(gets)<=3
            super().do_GET()
        def reply(self,status,data,mime='application/json; charset=utf-8',csp=None,headers=None):
            if malformed and self.path=='/academy':
                data=b"<!doctype html><script src src='/studio.js'></script><script-x src='/academy.js'></script-x>"
            super().reply(status,data,mime,csp,headers)
    lab.RequestHandlerClass=Counted
    worker=threading.Thread(target=lab.serve_forever,daemon=True);worker.start()
    base=f'http://127.0.0.1:{lab.server_port}';packets=[]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            packet = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            if reply_to_work_intent(self, packet):
                return
            packets.append(packet)
            step = len(packets)
            prior=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
            if step==1:args={'action':'fetch','url':base+'/academy'}
            elif step in (2,4,7):args={'action':'discover','artifact_id':prior[0]['artifact_id']}
            elif step in (3,5,6) and (scoped and not malformed or step==3):args={'action':'fetch_reference','artifact_id':prior[0]['artifact_id'],'reference_index':1 if step==6 else 0}
            else:args=None
            message=call('source_artifact',args,'source-'+str(step)) if args else {'role':'assistant','content':'Static source coverage only; browser loading unverified.'}
            payload=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
    model=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);mt=threading.Thread(target=model.serve_forever,daemon=True);mt.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-discovery-') as tmp:
            root=Path(tmp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            cmd=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{model.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','10','chat']
            scope=base+('/' if scoped else '/academy')
            prompts=f'/scope allow {scope} read\nDiscover local lab sources and collect only approved references.\ny\n'+('y\ny\n' if scoped and not malformed else '')+'/audit-report\n/exit\n'
            child=subprocess.run(cmd,env=env,cwd=workspace,input=prompts,text=True,capture_output=True,timeout=30)
            assert child.returncode==0,child.stderr[-2000:]
            files=list((app/'sessions').glob('*.json'));assert len(files)==1
            session=json.loads(files[0].read_text());results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool']
            assert len(results)==(7 if scoped and not malformed else 4)
            first=results[1]
            exported=audit_from_stdout(child.stdout)
            assert len(exported['roots'])==1 and exported['network_requests']==0
            root_report=exported['roots'][0]
            assert root_report['root_artifact_id']==results[0]['artifact_id']
            assert root_report['evidence_id']==results[-1]['evidence_id']
            assert root_report['runtime_coverage_complete'] is False
            assert root_report['current_cache_revalidated'] is False
            # Reopen the actual checkpoint, revoke scope, and export without model/network calls.
            before=len(gets);model_calls=len(packets)
            report_cmd=cmd[:-1]+['--resume',session['id'],'chat']
            changed=subprocess.run(report_cmd,env=env,cwd=workspace,input='/scope clear\n/audit-report\n/exit\n',text=True,capture_output=True,timeout=15)
            assert changed.returncode==0
            revoked=audit_from_stdout(changed.stdout)
            assert all(r['current_read_scope_authorized'] is False for r in revoked['roots'][0]['references'] if r['target_redacted'])
            assert len(gets)==before and len(packets)==model_calls
            # Corrupt only recorded discovery results, leaving their native digests unchanged.
            altered=json.loads(files[0].read_text())
            for message in altered['messages']:
                if message['role']!='tool':continue
                value=json.loads(message['content'])
                if 'root_artifact_id' in value and 'references' in value:
                    value['limited']=not value['limited'];message['content']=json.dumps(value)
            files[0].write_text(json.dumps(altered))
            checked=subprocess.run(report_cmd,env=env,cwd=workspace,input='/audit-report\n/exit\n',text=True,capture_output=True,timeout=15)
            assert checked.returncode==0
            invalid=audit_from_stdout(checked.stdout)
            assert invalid['roots']==[] and invalid['rejected_evidence']>=2
            assert len(gets)==before and len(packets)==model_calls
            if malformed:
                assert first['references']==[] and first['limited'] is True
                assert first['incomplete_script_tags']==1 and results[2]['ok'] is False
                assert gets==['/academy']
                print(json.dumps({'status':'passed','scope':'all_lab_sources','case':'malformed_duplicate_src_and_custom_element','lab_GETs':1,'dependency_GETs':0,'audit_export_verified':True,'revoked_scope_rechecked':True,'tampered_discovery_rejected':True,'report_GETs':0,'limited':True,'discovered_static_sources':0,'oracle':'conservative_duplicate_rejection_and_exact_element_name_contract','browser_loading_verified':False,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
                return
            assert len(first['references'])==len(expected)==2
            assert [r['target_redacted'] for r in first['references']]==[base+p for p in expected]
            assert first['network_requests']==0 and first['runtime_coverage_complete'] is False
            assert first['limited'] is False
            if scoped:
                assert gets==['/academy']+expected,gets
                assert first['counts']['unique_missing_in_scope']==2
                assert results[3]['counts']['unique_missing_in_scope']==1 and results[3]['counts']['unique_index_matches']==1
                assert results[4]['reused_local_copy'] is True and results[4]['fresh_evidence'] is False
                assert results[6]['counts']['unique_missing_in_scope']==0 and results[6]['counts']['unique_index_matches']==2
            else:
                assert gets==['/academy'] and results[2]['ok'] is False
                assert first['counts']['outside_scope']==2
                assert all(not r['fetch_available'] for r in first['references'])
            print(json.dumps({'status':'passed','scope':'all_lab_sources' if scoped else 'page_only','lab_GETs':len(gets),'discovered_static_sources':2,'initial_counts':first['counts'],'final_counts':results[-1]['counts'],'duplicate_fetch_GETs':0,'audit_export_verified':True,'revoked_scope_rechecked':True,'tampered_discovery_rejected':True,'report_GETs':0,'oracle':'independent_HTMLParser_on_pinned_lab_page','browser_loading_verified':False,'evidence_ids':[r['evidence_id'] for r in results],'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        lab.shutdown();lab.server_close();worker.join(timeout=3);model.shutdown();model.server_close();mt.join(timeout=3)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--lab',type=Path,required=True);a=p.parse_args();lab=a.lab.resolve()
    assert subprocess.check_output(['git','-C',str(lab),'rev-parse','HEAD'],text=True).strip()==LAB_COMMIT
    subprocess.run(['git','-C',str(lab),'diff','--quiet','HEAD','--'],check=True)
    oracle=ScriptOracle();oracle.feed((lab/'academy.html').read_text());oracle.close()
    sys.dont_write_bytecode=True;sys.addaudithook(loopback_only);sys.path.insert(0,str(lab));from app import make_server
    for scoped in (True,False):run(a.binary.resolve(),make_server,oracle.sources,scoped)
    run(a.binary.resolve(),make_server,oracle.sources,True,malformed=True)
