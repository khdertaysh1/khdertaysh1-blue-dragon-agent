#!/usr/bin/env python3
"""Real CLI cross-file investigation on explicit module fixtures in pinned lab server.
The three modules are test-only response adapters, not original lab vulnerabilities.
"""
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

def report_from_stdout(text):
    decoder=json.JSONDecoder()
    for at,char in enumerate(text):
        if char!='{':continue
        try:value,_=decoder.raw_decode(text[at:])
        except json.JSONDecodeError:continue
        if isinstance(value,dict) and value.get('schema')==2 and 'source_investigations' in value:return value['source_investigations']
    raise AssertionError('investigation report missing')

def case(binary,make_server,mode):
    prop='innerHTML' if mode=='weak_fixture' else 'textContent'
    caller="import {render} from './bridge.js'; const input=location.hash; "+("render=other; " if mode=='ambiguous_fixture' else '')+"render(input);"
    impl=f"export function show(value){{const normalized=value.trim(); node.{prop}=normalized;}}"
    if mode=='reassigned_export':impl += ' show=replacement;'
    if mode=='nested_export':impl = 'function outer(){function show(value){node.innerHTML=value;}} export const show=0;'
    if mode=='unexported_definition':impl = 'function show(value){node.innerHTML=value;}'
    blocked=mode in ('ambiguous_fixture','reassigned_export','nested_export','unexported_definition')
    sources={'/blue-bridge/app.js':caller,'/blue-bridge/bridge.js':"export {show as render} from './impl.js';",'/blue-bridge/impl.js':impl}
    lab=make_server(0);gets=[];original=lab.RequestHandlerClass
    class Adapter(original):
        def do_GET(self):
            assert self.valid_host()
            gets.append(self.path);assert len(gets)<=3
            if self.path in sources:return self.reply(200,sources[self.path].encode(),'text/javascript; charset=utf-8')
            raise AssertionError('Unexpected fixture request')
    lab.RequestHandlerClass=Adapter;worker=threading.Thread(target=lab.serve_forever,daemon=True);worker.start()
    base=f'http://127.0.0.1:{lab.server_port}';packets=[]
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(packet);step=len(packets)
            prior=[json.loads(m['content']) for m in packet['messages'] if m['role']=='tool']
            if step<=3:args={'action':'fetch','url':base+list(sources)[step-1]}
            elif step==4:args={'action':'search','artifact_id':prior[0]['artifact_id'],'query':'render(input)'}
            elif step==5:args={'action':'bridge_call','artifact_id':prior[0]['artifact_id'],'query':'render','offset':prior[3]['matches'][0]['offset'],'argument_index':0}
            elif step==6 and not blocked:args=next(s['read_suggestion'] for s in prior[4]['callee_flow']['steps'] if s['syntax']['target']=='node.'+prop)
            else:args=None
            message=call('source_artifact',args,'bridge-'+str(step)) if args else {'role':'assistant','content':'Syntactic correspondence only; execution and semantic dataflow unverified.'}
            body=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    model=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);mt=threading.Thread(target=model.serve_forever,daemon=True);mt.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-call-bridge-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            cmd=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{model.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','8','chat']
            child=subprocess.run(cmd,env=env,cwd=workspace,input=f'/scope allow {base}/blue-bridge/ read\nInspect the three source fixtures without executing them.\ny\ny\ny\n/exit\n',text=True,capture_output=True,timeout=30)
            assert child.returncode==0,child.stderr[-2000:]
            session=json.loads(next((app/'sessions').glob('*.json')).read_text());results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool']
            assert len(results)==(5 if blocked else 6)
            assert [r['artifact_id'] for r in results[:3]]==[hashlib.sha256(s.encode()).hexdigest() for s in sources.values()]
            result=results[4];assert result['network_requests']==0 and result['semantic_dataflow_verified'] is False
            if blocked:
                expected={'ambiguous_fixture':'possible_local_shadow_or_reassignment','reassigned_export':'possible_import_chain_binding_write','nested_export':'possible_import_chain_binding_write','unexported_definition':'definition_not_direct_named_module_export'}[mode]
                assert result['mapping_available'] is False and result['reason']==expected,result
            else:
                assert result['mapping_available'] is True and result['argument_symbol']=='input' and result['parameter_symbol']=='value'
                assert result['callee_symbol']=='show' and result['callee_artifact_id']==results[2]['artifact_id']
                endpoint=next(s for s in result['callee_flow']['steps'] if s['syntax']['target']=='node.'+prop)
                start,end=endpoint['syntax']['byte_offset'],endpoint['syntax']['end_byte']
                assert results[5]['excerpt']==impl.encode()[start:end].decode()
                assert results[5]['excerpt']==f'node.{prop}=normalized'
                assert len(result['import_chain']['nodes'])==3 and len(result['import_chain']['edges'])==2
            before_calls=len(packets)
            report_cmd=cmd[:-1]+['--resume',session['id'],'chat']
            exported=subprocess.run(report_cmd,env=env,cwd=workspace,input='/scope clear\n/audit-report\n/exit\n',text=True,capture_output=True,timeout=15)
            assert exported.returncode==0
            report=report_from_stdout(exported.stdout)
            assert len(report['rows'])==1 and report['rows'][0]['status']==('blocked' if blocked else 'syntactic_candidate'),report
            assert report['rows'][0]['caller_artifact_id']==results[0]['artifact_id']
            assert 'normalized' not in json.dumps(report) and 'node.innerHTML' not in json.dumps(report)
            checkpoint=next((app/'sessions').glob('*.json'));altered=json.loads(checkpoint.read_text())
            for message in altered['messages']:
                if message['role']!='tool':continue
                value=json.loads(message['content'])
                if 'mapping_available' in value:
                    value['mapping_available']=not value['mapping_available'];message['content']=json.dumps(value)
            checkpoint.write_text(json.dumps(altered))
            rejected=subprocess.run(report_cmd,env=env,cwd=workspace,input='/audit-report\n/exit\n',text=True,capture_output=True,timeout=15)
            assert rejected.returncode==0
            invalid=report_from_stdout(rejected.stdout)
            assert invalid['rows']==[] and invalid['rejected_evidence']>=1,invalid
            assert len(packets)==before_calls
            assert gets==list(sources)
            assert len(json.dumps(result))<24000
            return {'status':'passed','fixture':mode,'GETs':3,'analysis_GETs':0,'mapping_available':result['mapping_available'],'blocked_reason':result.get('reason'),'endpoint':None if blocked else 'node.'+prop,'source_hashes_verified':True,'endpoint_exact_read_verified':not blocked,'report_restart_and_tamper_rejection_verified':True,'report_model_calls':0,'semantic_dataflow_verified':False,'browser_execution_verified':False,'evidence_ids':[r['evidence_id'] for r in results]}
    finally:
        lab.shutdown();lab.server_close();worker.join(timeout=3);model.shutdown();model.server_close();mt.join(timeout=3)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--lab',type=Path,required=True);a=p.parse_args();lab=a.lab.resolve();binary=a.binary.resolve()
    assert subprocess.check_output(['git','-C',str(lab),'rev-parse','HEAD'],text=True).strip()==LAB_COMMIT
    subprocess.run(['git','-C',str(lab),'diff','--quiet','HEAD','--'],check=True)
    sys.dont_write_bytecode=True;sys.addaudithook(loopback_only);sys.path.insert(0,str(lab));from app import make_server
    rows=[case(binary,make_server,m) for m in ('weak_fixture','fixed_fixture','ambiguous_fixture','reassigned_export','nested_export','unexported_definition')]
    print(json.dumps({'status':'passed','lab_commit':LAB_COMMIT,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rows':rows,'GETs':sum(row['GETs'] for row in rows),'limits':'Synthetic module response adapters in pinned loopback lab. No JS execution, runtime dispatch or actual vulnerability/protection proof; Python guard is not an OS sandbox for Rust.'}))
