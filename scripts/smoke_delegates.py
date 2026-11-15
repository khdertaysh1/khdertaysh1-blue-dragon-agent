#!/usr/bin/env python3
"""Deterministic independent-agent concurrency, isolation and process-death recovery.
All provider traffic is loopback. This is not physical Android qualification.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time


def main(binary):
    class State:
        mode='parallel'; parent=0; child_calls={}; active=0; peak=0; errors=[]
        lock=threading.Lock(); both=threading.Event(); ready=threading.Event(); peer=threading.Event()
        kill_child=threading.Event(); kill_parent=threading.Event(); release=threading.Event()
    state=State()
    def call(name,args,ident):
        return {'role':'assistant','content':'Fixture operation','tool_calls':[{'id':ident,'type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_): pass
        def do_POST(self):
            try:
                packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                msgs=packet['messages']; child='independent read-only research delegate' in str(msgs[0]['content'])
                if child:
                    task=str(msgs[1]['content']); who='A' if 'TASK_A' in task else 'B'
                    with state.lock:
                        step=state.child_calls.get(who,0)+1;state.child_calls[who]=step
                        state.active+=1;state.peak=max(state.peak,state.active)
                        if state.active==2:state.both.set()
                    if state.mode=='swap':
                        if step==1:
                            original=workspace/'a.txt';original.unlink();original.mkdir()
                            (original/'PRIVATE_CHILD_NAME').write_text('private')
                            message=call('list_files',{'path':'a.txt'},'swap-list')
                            message['tool_calls'] += [call('file_info',{'path':'a.txt'},'swap-info')['tool_calls'][0],call('read_file_window',{'path':'a.txt/PRIVATE_CHILD_NAME'},'swap-read')['tool_calls'][0]]
                        else:
                            values=[json.loads(m['content']) for m in msgs if m['role']=='tool']
                            assert len(values)==3 and all(v.get('denied') is True for v in values),values
                            message={'role':'assistant','content':'Replaced file grant denied; no enumeration authorized.'}
                    elif state.mode in ('kill','cancel'):
                        state.kill_child.set();state.release.wait(20)
                        message=call('write_file',{'path':'CANCEL_FORBIDDEN','content':'bad'},'late-write')
                    else:
                        if step==1:
                            assert state.both.wait(8),'Children did not run concurrently'
                            assert state.ready.wait(8),'Parent message was not queued'
                            if who=='A':
                                message=call('read_file_window',{'path':'a.txt','byte_offset':200000,'byte_count':64},'read-a')
                                message['tool_calls'] += [call('write_file',{'path':'FORBIDDEN','content':'bad'},'write-a')['tool_calls'][0],call('read_file_window',{'path':'b.py','byte_offset':0,'byte_count':64},'escape-a')['tool_calls'][0],call('agent_message',{'to':'parent','message':'A read-only progress'},'note-a')['tool_calls'][0]]
                            else:
                                message=call('read_file_window',{'path':'b.py','byte_offset':0,'byte_count':64},'read-b')
                                message['tool_calls'] += [call('file_outline',{'path':'b.py'},'outline-b')['tool_calls'][0],call('file_outline',{'path':'a.txt'},'outline-outside')['tool_calls'][0]]
                        elif step==2 and who=='A':
                            peer=re.search(r'peer:([0-9A-Za-z-]+)',json.dumps(msgs)).group(1)
                            message=call('agent_message',{'to':peer,'message':'Peer observation from A'},'peer-a')
                            message['tool_calls'].append(call('search_file',{'path':'a.txt','query':'ALPHA_FILE_A','byte_offset':0,'scan_bytes':262144,'max_matches':1},'search-a')['tool_calls'][0])
                        elif step==2:
                            assert state.peer.wait(8),'Peer message not sent'
                            message=call('agent_message',{'to':'parent','message':'B progress after peer rendezvous'},'note-b')
                        else:
                            if who=='A':
                                state.peer.set()
                                search=next(json.loads(m['content']) for m in msgs if m['role']=='tool' and 'matches' in json.loads(m['content']))
                                assert search['matches'][0]['byte_offset']==200000
                            if who=='B':assert 'Peer observation from A' in json.dumps(msgs),'Peer inbox lost'
                            message={'role':'assistant','content':'Conclusion A: condition holds' if who=='A' else 'Conclusion B: condition does not hold'}
                    with state.lock:state.active-=1
                else:
                    state.parent+=1;step=state.parent
                    results=[json.loads(m['content']) for m in msgs if m['role']=='tool']
                    if state.mode=='kill':
                        if step==1:message=call('delegate_agents',{'action':'spawn','task':'TASK_A wait fixture','paths':['a.txt']},'kill-spawn')
                        else:
                            state.kill_parent.set();message=call('delegate_agents',{'action':'wait','timeout_ms':1000},f'kill-wait-{step}')
                    elif state.mode=='cancel':
                        if step==1:message=call('delegate_agents',{'action':'spawn','task':'TASK_A cancellation fixture','paths':['a.txt']},'cancel-spawn')
                        elif step==2:
                            assert state.kill_child.wait(8)
                            ident=next(r['agent_id'] for r in results if r.get('state')=='started')
                            message=call('delegate_agents',{'action':'cancel','agent_id':ident},'cancel-child')
                        else:
                            state.release.set()
                            reports=[r for r in results if 'agents' in r]
                            if reports and reports[-1]['agents'][0]['state']=='cancelled':message={'role':'assistant','content':'Cancellation confirmed.'}
                            else:message=call('delegate_agents',{'action':'wait','timeout_ms':1000-step},f'cancel-wait-{step}')
                    elif state.mode=='swap':
                        if step==1:message=call('delegate_agents',{'action':'spawn','task':'TASK_A inspect granted file','paths':['a.txt']},'swap-spawn')
                        else:
                            reports=[r for r in results if 'agents' in r]
                            if reports and reports[-1]['agents'][0]['state']=='completed':message={'role':'assistant','content':'Replaced path was denied.'}
                            else:message=call('delegate_agents',{'action':'wait','timeout_ms':1000-step},f'swap-wait-{step}')
                    elif state.mode=='resume':
                        message=call('task_continuity',{},'resume-inspect') if step==1 else {'role':'assistant','content':'Historical interrupted delegate remains stopped.'}
                    elif step<=2:
                        who='A' if step==1 else 'B'
                        message=call('delegate_agents',{'action':'spawn','task':f'TASK_{who} inspect only assigned file','paths':['a.txt' if who=='A' else 'b.py']},f'spawn-{who}')
                    elif step==3:
                        ids=[r['agent_id'] for r in results if r.get('state')=='started']
                        assert len(ids)==2
                        message=call('delegate_agents',{'action':'send','agent_id':ids[0],'message':'peer:'+ids[1]},'parent-mail')
                    else:
                        state.ready.set()
                        reports=[r for r in results if 'agents' in r]
                        done=reports and len(reports[-1]['agents'])==2 and all(a['state']=='completed' for a in reports[-1]['agents'])
                        if done:message={'role':'assistant','content':'Both independent results collected; contradiction retained for parent review.'}
                        else:message=call('delegate_agents',{'action':'wait','timeout_ms':1000-(step-4)},f'wait-{step}')
                body=json.dumps({'choices':[{'message':message}]}).encode()
                self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            except (BrokenPipeError,ConnectionResetError): pass
            except Exception as e:
                state.errors.append(str(e));self.send_error(500)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-delegates-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            (workspace/'a.txt').write_text('x'*200000+'ALPHA_FILE_A');(workspace/'b.py').write_text("def worker_b():\n    return 'BETA_FILE_B'\n")
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),NO_COLOR='1',TERM='dumb')
            args=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','16']
            result=subprocess.run(args+['chat'],input='Run two read-only delegates.\ny\ny\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=35)
            assert result.returncode==0,result.stderr[-3000:]
            assert not state.errors,state.errors
            assert state.peak==2,state.peak
            parent_paths=list((app/'sessions').glob('*.json'));assert len(parent_paths)==1
            parent=json.loads(parent_paths[0].read_text())
            results=[json.loads(m['content']) for m in parent['messages'] if m['role']=='tool']
            report=[r for r in results if 'agents' in r][-1]
            assert all(a['state']=='completed' for a in report['agents']),report
            assert len({a['session_id'] for a in report['agents']})==2
            assert {a['summary'] for a in report['agents']}=={'Conclusion A: condition holds','Conclusion B: condition does not hold'}
            assert report['shared_provider_requests_remaining']==18,report
            assert not (workspace/'FORBIDDEN').exists()
            assert report['automatic_patch_merge'] is False
            assert all(e['session_id']==a['session_id'] for a in report['agents'] for e in a['evidence'])
            child_dir=app/'sessions'/'delegates'/parent['id']
            children=[json.loads(p.read_text()) for p in child_dir.glob('*.json') if p.name!='budget.json']
            assert len(children)==2
            a=next(c for c in children if 'TASK_A' in c['messages'][1]['content'])
            assert 'BETA_FILE_B' not in json.dumps(a)
            native=[json.loads(m['content']) for m in a['messages'] if m['role']=='tool']
            assert sum(r.get('denied') is True for r in native)==2,native
            b=next(c for c in children if 'TASK_B' in c['messages'][1]['content'])
            bresults=[json.loads(m['content']) for m in b['messages'] if m['role']=='tool']
            outline=next(r for r in bresults if 'source_sha256' in r)
            assert outline['language']=='Python' and outline['items'][0]['name']=='worker_b',outline
            assert sum(r.get('denied') is True for r in bresults)==1,bresults
            assert any(e['tool']=='file_outline' and e['session_id']==b['id'] for agent in report['agents'] for e in agent['evidence'])

            # Reconcile every output/message byte against native saved transcripts.
            charged=sum(len(m['content'].encode()) for c in children for m in c['messages'] if m['role']=='tool')
            charged+=sum(len(a['summary'].encode()) for a in report['agents'])
            for c in children+[parent]:
                for m in c['messages']:
                    for tool in m.get('tool_calls',[]):
                        f=tool['function'];a=json.loads(f['arguments'])
                        if f['name']=='agent_message' or f['name']=='delegate_agents' and a.get('action')=='send':
                            charged+=len(a['message'].encode())
            assert report['shared_output_bytes_remaining']==65536-charged,(report,charged)
            saved_budget=json.loads((child_dir/'budget.json').read_text())
            assert saved_budget['output']==65536-charged,saved_budget
            # Replace a granted file with a directory before the first native tool.
            prior_paths=set((app/'sessions').glob('*.json'))
            state.mode='swap';state.parent=0;state.child_calls={}
            swapped=subprocess.run(args+['chat'],input='Inspect the single granted file.\ny\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15)
            assert swapped.returncode==0,swapped.stderr
            swap_path=next(p for p in (app/'sessions').glob('*.json') if p not in prior_paths)
            swap_session=json.loads(swap_path.read_text())
            swap_dir=app/'sessions'/'delegates'/swap_session['id']
            swap_child=json.loads(next(p for p in swap_dir.glob('*.json') if p.name!='budget.json').read_text())
            swap_results=[json.loads(m['content']) for m in swap_child['messages'] if m['role']=='tool']
            assert len(swap_results)==3 and all(v.get('denied') is True for v in swap_results),swap_results
            assert 'PRIVATE_CHILD_NAME' not in json.dumps(swap_results)
            (workspace/'a.txt'/'PRIVATE_CHILD_NAME').unlink();(workspace/'a.txt').rmdir();(workspace/'a.txt').write_text('restored fixture')
            # Cooperative cancellation during an in-flight provider response.
            state.mode='cancel';state.parent=0;state.child_calls={}
            cancelled=subprocess.run(args+['chat'],input='Start then cancel one delegate.\ny\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15)
            assert cancelled.returncode==0,cancelled.stderr
            assert state.child_calls=={'A':1},state.child_calls
            assert not (workspace/'CANCEL_FORBIDDEN').exists()
            prior_paths=set((app/'sessions').glob('*.json'))
            state.kill_child.clear();state.kill_parent.clear();state.release.clear()
            # New parent session: kill while its child provider request is in flight.
            state.mode='kill';state.parent=0;state.child_calls={}
            proc=subprocess.Popen(args+['chat'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,text=True,env=env,cwd=workspace)
            proc.stdin.write('Start one delegate.\ny\n');proc.stdin.flush()
            assert state.kill_child.wait(8) and state.kill_parent.wait(8)
            proc.kill();proc.wait(timeout=5);proc.stdin.close()
            kill_paths=[p for p in (app/'sessions').glob('*.json') if p not in prior_paths];assert len(kill_paths)==1
            killed=json.loads(kill_paths[0].read_text());kill_id=killed['id']
            budget=json.loads((app/'sessions'/'delegates'/kill_id/'budget.json').read_text())
            assert budget['requests']==23,budget
            calls_before=dict(state.child_calls);state.mode='resume';state.parent=0
            resumed=subprocess.run(args+['--resume',kill_id,'chat'],input='Inspect continuity only.\n/exit\n',text=True,capture_output=True,env=env,cwd=workspace,timeout=15)
            assert resumed.returncode==0,resumed.stderr
            saved=json.loads(kill_paths[0].read_text())
            reports=[json.loads(m['content']) for m in saved['messages'] if m['role']=='tool']
            continuity=next(r for r in reversed(reports) if 'delegates' in r)
            assert continuity['delegates']['agents'][0]['state']=='interrupted',continuity
            assert continuity['delegates']['shared_provider_requests_remaining']==23
            assert state.child_calls==calls_before,'Interrupted child was replayed'
            assert not state.errors,state.errors
            print(json.dumps({'status':'passed','independent_agents':2,'peak_concurrency':state.peak,'child_provider_calls_parallel_case':6,'forbidden_operations_denied':3,'scoped_source_outline':True,'peer_messages':True,'all_message_routes_budget_reconciled':True,'file_to_directory_grant_denied':True,'contradictory_summaries_preserved':True,'process_kill_recovery':'interrupted/no replay','inflight_cancellation':'no tools executed after cancellation','large_file_byte_offset':200000,'phone_tested':False,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        state.release.set();server.shutdown();server.server_close();thread.join(timeout=3)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);main(p.parse_args().binary.resolve())
