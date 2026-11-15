#!/usr/bin/env python3
"""Actual independent-agent source reconciliation, including restart and tampering."""
import argparse
import copy
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def main(binary):
    class State:
        mode='different';parent=0;children={};serial=0;errors=[];ready=threading.Event();lock=threading.Lock()
    state=State()
    def call(name,args):
        with state.lock:state.serial+=1;ident=state.serial
        return {'role':'assistant','tool_calls':[{'id':f'c{ident}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            try:
                packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));messages=packet['messages']
                child='independent read-only research delegate' in str(messages[0]['content'])
                if child:
                    who='A' if 'TASK_A' in str(messages[1]['content']) else 'B'
                    with state.lock:step=state.children.get(who,0)+1;state.children[who]=step
                    if step==1:
                        if who=='B':assert state.ready.wait(8),'A did not finish its source capture'
                        message=call('file_outline',{'path':'shared.py'})
                    else:
                        assert any('source_sha256' in json.loads(m['content']) for m in messages if m['role']=='tool')
                        if who=='A':
                            if state.mode=='different':(workspace/'shared.py').write_text('def changed_source():\n    return 2\n')
                            state.ready.set()
                        message={'role':'assistant','content':'Same conclusion; do not infer source agreement from this sentence.'}
                else:
                    state.parent+=1;step=state.parent
                    results=[json.loads(m['content']) for m in messages if m['role']=='tool']
                    reports=[r for r in results if 'agents' in r]
                    if state.mode=='peek':
                        if step==1:message=call('delegate_agents',{'action':'collect'})
                        elif step==2:
                            agent=reports[-1]['agents'][0];evidence=agent['evidence'][0]
                            message=call('delegate_agents',{'action':'inspect','agent_id':agent['session_id'],'evidence_id':evidence['evidence_id'],'expected_result_sha256':'0'*64 if state.wrong_pin else evidence['result_sha256'],'byte_count':1024})
                        else:message={'role':'assistant','content':'Historical diagnostic inspected without replay.'}
                    elif state.mode=='inspect':message=call('delegate_agents',{'action':'collect'}) if step==1 else {'role':'assistant','content':'Historical captures inspected without replay.'}
                    elif step<=2:message=call('delegate_agents',{'action':'spawn','task':f'TASK_{"A" if step==1 else "B"} inspect shared source only','paths':['shared.py']})
                    elif reports and len(reports[-1]['agents'])==2 and all(a['state']=='completed' for a in reports[-1]['agents']):message={'role':'assistant','content':'Review the native capture versions before combining conclusions.'}
                    else:message=call('delegate_agents',{'action':'wait','timeout_ms':1000-step})
                body=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            except Exception as e:state.errors.append(str(e));self.send_error(500)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-source-consistency-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state';(workspace/'shared.py').write_text('def original_source():\n    return 1\n')
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            command=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','12']
            def report(path):
                session=json.loads(path.read_text());return next(json.loads(m['content']) for m in reversed(session['messages']) if m['role']=='tool' and 'agents' in json.loads(m['content']))
            for mode,expected in [('different','captured_versions_differ'),('same','same_captured_version')]:
                state.mode=mode;state.parent=0;state.children={};state.ready.clear()
                previous=set((app/'sessions').glob('*.json'))
                run=subprocess.run(command+['chat'],input='Compare two independent source captures.\ny\ny\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=25)
                assert run.returncode==0,run.stderr;assert not state.errors,state.errors
                path=next(p for p in (app/'sessions').glob('*.json') if p not in previous);result=report(path);row=result['source_consistency']['files'][0]
                assert row['state']==expected and row['agent_count']==2,row
                assert len({a['summary'] for a in result['agents']})==1
                assert row['semantic_agreement_verified'] is False and row['current_file_verified'] is False
                if mode=='different':changed_path=path
            before=dict(state.children);state.mode='inspect';state.parent=0
            ident=json.loads(changed_path.read_text())['id']
            def inspect():
                state.parent=0
                run=subprocess.run(command+['--resume',ident,'chat'],input='Inspect historical captures only.\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15);assert run.returncode==0,run.stderr
                assert state.children==before,'Historical source capture was replayed'
                return report(changed_path)
            def exported(expected_count):
                before_parent=state.parent;before_children=dict(state.children)
                run=subprocess.run(command+['--resume',ident,'chat'],input='/audit-report\n/report\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15)
                assert run.returncode==0,run.stderr
                assert state.parent==before_parent and state.children==before_children,'Report contacted provider'
                reports=[];decoder=json.JSONDecoder()
                for start,char in enumerate(run.stdout):
                    if char!='{':continue
                    try:value,_=decoder.raw_decode(run.stdout[start:])
                    except ValueError:continue
                    if isinstance(value,dict) and 'delegate_evidence' in value:reports.append(value['delegate_evidence'])
                assert len(reports)==2,run.stdout
                for report in reports:
                    assert sum(len(a['evidence']) for a in report['agents'])==expected_count,report
                    assert report['actions_replayed']==0 and report['raw_results_included'] is False
                    assert report['model_summaries_included'] is False
                    assert 'Same conclusion' not in json.dumps(report)
                return reports
            restored=inspect();assert restored['source_consistency']['files'][0]['state']=='captured_versions_differ'
            exported(2)
            child_dir=app/'sessions'/'delegates'/ident;child_path=next(p for p in child_dir.glob('*.json') if p.name!='budget.json');child=json.loads(child_path.read_text())
            pristine=copy.deepcopy(child)
            for m in child['messages']:
                if m['role']=='tool':value=json.loads(m['content']);value['source_sha256']='0'*64;m['content']=json.dumps(value);break
            child_path.write_text(json.dumps(child));tampered=inspect()
            assert tampered['source_consistency']['files'][0]['state']=='single_agent_capture',tampered
            assert tampered['source_consistency']['unverified_or_omitted_child_evidence']==1,tampered
            exported(1)
            peer_path=next(p for p in child_dir.glob('*.json') if p.name!='budget.json' and p!=child_path)
            peer=json.loads(peer_path.read_text())
            peer_hash=next(json.loads(m['content'])['source_sha256'] for m in peer['messages'] if m['role']=='tool')
            for tamper in [False,True]:
                for missing in [('request_sha256',), ('result_sha256',), ('request_sha256','result_sha256')]:
                    downgraded=copy.deepcopy(pristine)
                    for observation in downgraded['observations']:
                        for field in missing:observation.pop(field,None)
                    if tamper:
                        for m in downgraded['messages']:
                            if m['role']=='tool':
                                value=json.loads(m['content']);value['source_sha256']=peer_hash;m['content']=json.dumps(value)
                    child_path.write_text(json.dumps(downgraded));rejected=inspect()
                    assert rejected['source_consistency']['files'][0]['state']=='single_agent_capture',rejected
                    assert rejected['source_consistency']['unverified_or_omitted_child_evidence']==1,rejected
            child_path.write_text(json.dumps(pristine));valid_again=inspect()
            assert valid_again['source_consistency']['files'][0]['state']=='captured_versions_differ'
            assert valid_again['source_consistency']['unverified_or_omitted_child_evidence']==0
            exported(2)
            (workspace/'shared.py').unlink()
            for wrong_pin in [False,True]:
                state.mode='peek';state.parent=0;state.wrong_pin=wrong_pin
                run=subprocess.run(command+['--resume',ident,'chat'],input='Inspect a recorded child result only.\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15)
                assert run.returncode==0,run.stderr
                assert state.children==before,'Inspection restarted child'
                saved=json.loads(changed_path.read_text())
                value=next(json.loads(m['content']) for m in reversed(saved['messages']) if m['role']=='tool')
                if wrong_pin:assert value['ok'] is False,value
                else:
                    assert value['historical'] is True and value['actions_replayed']==0,value
                    assert value['trusted_instruction'] is False and value['current_source_verified'] is False
                    assert value['recorded_outcome']=='observed_return' and 'source_sha256' in value['excerpt']
                    assert len(value['excerpt'].encode())<=1024
            assert not state.errors,state.errors
            print(json.dumps({'status':'passed','different_versions_detected':True,'identical_summary_not_consensus':True,'same_capture_detected':True,'tampered_capture_excluded':True,'missing_digest_downgrades_rejected':6,'valid_pinned_restore_accepted':True,'report_exports_revalidated_without_provider':True,'pinned_inspection_after_source_deletion':True,'wrong_inspection_pin_rejected':True,'restart_child_requests':0,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:server.shutdown();server.server_close();thread.join(timeout=3)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);main(p.parse_args().binary.resolve())
