#!/usr/bin/env python3
"""Actual read/edit/read workflows with independent on-disk byte oracles.
Synthetic loopback provider, fresh homes, normal CLI approval settings.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import re
import stat
import subprocess
import tempfile
import threading
import time
from benchmark_alpha_reads import validate_result

BASE = b'port=8000\nmode=dev\n'
CHANGED = b'port=8000\nmode=prod\n'
SIBLING = b'UNRELATED_FILE_MUST_STAY_UNCHANGED\n'
CASES = ('unique', 'utf8', 'absent', 'ambiguous', 'recovery')


def plan(case):
    # Expected states are independently specified fixtures, not computed by the edit implementation.
    initial = BASE
    if case == 'utf8': initial = 'عنوان=قديم 🐉\n'.encode()
    if case == 'ambiguous': initial = b'mode=dev\nmode=dev\n'
    steps = [('read', {}, initial, None)]
    if case in ('absent', 'recovery'):
        steps.append(('edit', {'old': 'NEVER_PRESENT_71f', 'new': 'replacement'}, initial, 'absent'))
    if case == 'ambiguous':
        steps.append(('edit', {'old': 'mode=dev', 'new': 'mode=prod'}, initial, 'ambiguous'))
    elif case == 'utf8':
        steps.append(('edit', {'old': 'قديم 🐉', 'new': 'جديد 🐉'}, 'عنوان=جديد 🐉\n'.encode(), None))
    elif case in ('unique', 'recovery'):
        steps.append(('edit', {'old': 'mode=dev', 'new': 'mode=prod'}, CHANGED, None))
    steps.append(('read', {}, steps[-1][2], None))
    return initial, steps


def digest(data): return hashlib.sha256(data).hexdigest()


def file_digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda: f.read(65536), b''): h.update(chunk)
    return h.hexdigest()


def assert_disk(workspace, expected):
    for name, body in [('config.txt',expected),('unrelated.txt',SIBLING)]:
        path=workspace/name
        assert stat.S_ISREG(path.lstat().st_mode), 'Fixture path type changed'
        with path.open('rb') as f: actual=f.read(len(body)+1)
        assert actual==body, 'Independent byte oracle failed for '+name


def validate_step(text, engine, kind, expected, rejection):
    if rejection is None:
        # Reuse the corrected status gate; model prose alone is never sampled here.
        validate_result(text, engine, 'small', expected.decode() if kind == 'read' else '')
        if kind == 'read':
            try: value = json.loads(text)
            except ValueError: value = None
            if engine == 'blue':
                assert isinstance(value, dict) and value.get('content') == expected.decode(), 'Readback bytes differ'
        return
    try: value = json.loads(text)
    except ValueError: value = None
    # A negative case must reach the intended native match check, not fail for permission or uncertainty.
    if isinstance(value, dict):
        for field in ('denied', 'cached', 'cached_result', 'reused_local_copy', 'stale'):
            assert value.get(field) is not True, 'Blocked/stale result is not an edit-match rejection'
        assert value.get('execution_status') != 'unknown' and value.get('fresh_evidence') is not False
        assert 'original_evidence_id' not in value and 'reused_evidence_id' not in value
        assert value.get('ok') is not True, 'Successful result cannot establish rejected edit'
        error = str(value.get('error', ''))
        if engine == 'blue': assert value.get('ok') is False
        status = str(value.get('status', '')).lower()
        assert not any(x in status for x in ('denied', 'unknown', 'cached', 'stale', 'blocked'))
    else: error = text
    lower = error.lower()
    assert not any(x in lower for x in ('permission', 'denied', 'cached', 'stale', 'unknown')), 'Unrelated error'
    if rejection == 'absent':
        assert 'exactly one match' in lower or 'old_string not found' in lower, 'Not an absent-match rejection'
    else:
        assert 'exactly one match' in lower or re.search(r'old_string found [2-9][0-9]* times', lower), 'Not an ambiguous-match rejection'


def run_case(binary, engine, case):
    initial, steps = plan(case)
    observed, errors = [], []
    requests = 0
    with tempfile.TemporaryDirectory(prefix='blue-alpha-workflow-') as temp:
        root = Path(temp); workspace = root/'workspace'; workspace.mkdir(); home = root/'home'; home.mkdir()
        (workspace/'config.txt').write_bytes(initial); (workspace/'unrelated.txt').write_bytes(SIBLING)
        def operation(index):
            kind, arg, _, _ = steps[index]
            if engine == 'alpha':
                return ('read', {'file_path':'config.txt','offset':0,'limit':20}) if kind=='read' else ('edit', {'file_path':'config.txt','old_string':arg['old'],'new_string':arg['new'],'replace_all':False})
            return ('read_file', {'path':'config.txt'}) if kind=='read' else ('edit_file', {'path':'config.txt','find':arg['old'],'replace':arg['new']})
        class Model(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_POST(self):
                nonlocal requests
                try:
                    size = int(self.headers.get('Content-Length','0')); assert 0 < size <= 4*1024*1024
                    packet = json.loads(self.rfile.read(size)); requests += 1
                    assert requests <= len(steps)+2, 'Unexpected retry or extra model request'
                    index = requests-1
                    if 0 < index <= len(steps):
                        prior = index-1; kind, _, expected, rejection = steps[prior]
                        paired = [m['content'] for m in packet['messages'] if m.get('role')=='tool' and m.get('tool_call_id')==f'workflow_{prior}']
                        assert len(paired)==1, 'Missing or duplicate paired native result'
                        text = paired[0] if isinstance(paired[0],str) else json.dumps(paired[0])
                        validate_step(text,engine,kind,expected,rejection)
                        assert_disk(workspace,expected)
                        observed.append({'step':prior,'kind':kind,'expected_rejection':rejection,'result_sha256':digest(text.encode()),'target_sha256':digest(expected),'sibling_sha256':digest(SIBLING)})
                    elif index == 0: assert_disk(workspace,initial)
                    else: assert_disk(workspace,steps[-1][2])
                    if index < len(steps):
                        name,args = operation(index)
                        advertised = {t['function']['name'] for t in packet.get('tools',[]) if t.get('type')=='function'}
                        assert name in advertised, 'Adapter tool was not advertised'
                        message = {'role':'assistant','content':None,'tool_calls':[{'id':f'workflow_{index}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
                    else: message = {'role':'assistant','content':'Synthetic workflow complete. No general parity claim.'}
                    if packet.get('stream'):
                        delta = dict(message);delta.pop('role',None)
                        for i,c in enumerate(delta.get('tool_calls',[])):c['index']=i
                        chunk={'id':'fixture','object':'chat.completion.chunk','choices':[{'index':0,'delta':delta,'finish_reason':None}]}
                        finish={'id':'fixture','object':'chat.completion.chunk','choices':[{'index':0,'delta':{},'finish_reason':'tool_calls' if 'tool_calls' in message else 'stop'}]}
                        body=('data: '+json.dumps(chunk)+'\n\ndata: '+json.dumps(finish)+'\n\ndata: [DONE]\n\n').encode();mime='text/event-stream'
                    else:
                        body=json.dumps({'choices':[{'index':0,'message':message,'finish_reason':'tool_calls' if 'tool_calls' in message else 'stop'}]}).encode();mime='application/json'
                    self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
                except Exception as exc: errors.append(str(exc));self.send_error(500)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
        env.update(HOME=str(home),USERPROFILE=str(home),ALPHACODE_HOME=str(home/'alpha'),BLUE_DRAGON_HOME=str(home/'blue'),XDG_CONFIG_HOME=str(home/'config'),XDG_DATA_HOME=str(home/'data'),XDG_CACHE_HOME=str(home/'cache'),TERM='dumb',NO_COLOR='1')
        endpoint=f'http://127.0.0.1:{server.server_port}/v1'
        prompt='Execute only the synthetic config.txt read/edit/read fixture. Edits of config.txt are authorized; preserve unrelated.txt and all other files.'
        if engine=='alpha':
            env.update(ALPHACODE_OPENAI_COMPAT_API_BASE=endpoint,OPENAI_COMPAT_API_KEY='synthetic-fixture-key',ALPHACODE_RUN_MCP='0',ALPHACODE_RUN_AUTO_POKE='0',ALPHACODE_OPENROUTER_CONTEXT_WINDOW='65536')
            command=[str(binary),'--provider','openai-compatible','--model','fixture','--no-update','--no-selfdev','-C',str(workspace),'run','--json',prompt]
        else:command=[str(binary),'--root',str(workspace),'--base-url',endpoint,'--model','fixture','--no-stream','--no-rotation','--max-steps','8','run',prompt]
        started=time.perf_counter()
        try:
            # Answer normal diff prompts for these specifically authorized synthetic edits; no auto-approve flag.
            run=subprocess.run(command,input='y\n'*4,text=True,capture_output=True,env=env,cwd=workspace,timeout=45)
            assert not errors, errors
            assert run.returncode==0, f'CLI exit {run.returncode}: {run.stderr[-1000:]}'
            assert len(observed)==len(steps), 'Workflow ended before all native results were checked'
            assert_disk(workspace,steps[-1][2])
            return {'status':'passed','provider_requests':requests,'completion_only_followups':requests-len(steps)-1,'steps':observed,'elapsed_ms':round((time.perf_counter()-started)*1000,3)}
        finally:server.shutdown();server.server_close();thread.join(timeout=3)


def measure(binary, engine, repeats):
    before=file_digest(binary); rows=[]
    for case in CASES:
        for repetition in range(repeats):
            try: sample=run_case(binary,engine,case)
            except Exception as exc: sample={'status':'failed_or_adapter_blocked','reason':str(exc)[:2000]}
            rows.append({'case':case,'repetition':repetition+1,**sample})
    assert file_digest(binary)==before, 'Executable changed during measurement'
    return {'binary_sha256':before,'cases':rows}


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--blue',type=Path,required=True);p.add_argument('--alpha',type=Path);p.add_argument('--repeats',type=int,default=1);p.add_argument('--output',type=Path,required=True);args=p.parse_args()
    if not 1<=args.repeats<=3:p.error('repeats must be 1..3')
    with args.output.open('x') as output:
        report={'schema':1,'scope':'Five deterministic edit workflows, not autonomous problem solving or overall parity','platform':platform.platform(),'harness_sha256':file_digest(__file__),'read_oracle_sha256':file_digest(Path(__file__).with_name('benchmark_alpha_reads.py')),'blue':measure(args.blue.resolve(),'blue',args.repeats),'alpha':measure(args.alpha.resolve(),'alpha',args.repeats) if args.alpha else {'status':'not_run'},'limits':['Normal approval settings; affirmative stdin answers only authorized fixture edits. No approval-equivalence claim.','On-disk target and sibling checked after each native step; no OS isolation or exhaustive filesystem audit.','Alpha uses its documented low-memory build; timing is not a speed ranking.','No live model, browser, phone or semantic capability comparison.']}
        json.dump(report,output,indent=2);output.write('\n')
    print(json.dumps({e:sum(x['status']=='passed' for x in report[e].get('cases',[])) for e in ['blue','alpha']}))
    raise SystemExit(0 if all(x['status']=='passed' for e in ['blue','alpha'] for x in report[e].get('cases',[])) else 1)
