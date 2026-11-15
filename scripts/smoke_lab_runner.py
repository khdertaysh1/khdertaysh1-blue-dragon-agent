#!/usr/bin/env python3
"""Actual Blue Dragon integrated runner against pinned Between Bytes loopback lab.
Test-only response adapters supply an unstable control, truncation or disconnect.
No browser execution, original checkout edits, or external destinations.
"""
import argparse
import hashlib
import http.server
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
from urllib.parse import parse_qs, urlsplit
from lab_matrix import LAB_COMMIT, loopback_only
from smoke_continuity import call


def lab_report_from_stdout(text):
    decoder=json.JSONDecoder()
    for at,char in enumerate(text):
        if char!='{':continue
        try:value,_=decoder.raw_decode(text[at:])
        except json.JSONDecodeError:continue
        if isinstance(value,dict) and value.get('schema')==2 and 'local_lab_comparisons' in value:return value['local_lab_comparisons']
    raise AssertionError('local lab report missing')

class EncodingMismatch(AssertionError):
    pass

class ProbeParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.probe_tags=0
        self.escaped_probe_text=False
    def handle_starttag(self, tag, attrs):
        if tag=='blue-probe':
            self.probe_tags+=1
    def handle_data(self, data):
        if '<blue-probe>' in data:
            self.escaped_probe_text=True


def case(binary, make_server, mode, condition, scenario='literal_marker'):
    lab = make_server(0)
    paths = []
    wire_observations = []
    parsed_oracles = []
    original = lab.RequestHandlerClass
    class Adapter(original):
        def do_GET(self):
            paths.append(self.path.split('?')[0])
            assert len(paths) <= 3, 'Request budget exceeded or replay occurred'
            if condition == 'disconnect':
                self.close_connection = True
                return
            super().do_GET()
        def reply(self, status, data, mime='application/json; charset=utf-8', csp=None, headers=None):
            if isinstance(data, bytes) and condition == 'mixed_encoding':
                data += parse_qs(urlsplit(self.path).query)['q'][0].encode()
            if isinstance(data, bytes) and condition == 'escape_removed':
                data=data.replace(b'&lt;blue-probe&gt;',b'<blue-probe>').replace(b'&lt;/blue-probe&gt;',b'</blue-probe>')
            if isinstance(data, bytes) and condition == 'unstable' and len(paths) == 3:
                data += b'<!-- deliberately changed repeated control -->'
            if isinstance(data, bytes) and condition == 'truncated':
                data += b'x' * 65537
            raw=data if isinstance(data,bytes) else json.dumps(data).encode()
            parser=ProbeParser()
            parser.feed(raw.decode('utf-8'))
            parser.close()
            parsed_oracles.append({'probe_tags':parser.probe_tags,'escaped_probe_text':parser.escaped_probe_text})
            wire_observations.append({'status':status,'body_sha256':hashlib.sha256(raw[:65536]).hexdigest(),'content_type_sha256':hashlib.sha256(mime.encode()).hexdigest(),'bytes_captured':min(len(raw),65536),'complete':len(raw)<=65536})
            super().reply(status, data, mime, csp, headers)
    lab.RequestHandlerClass = Adapter
    worker = threading.Thread(target=lab.serve_forever, daemon=True)
    worker.start()
    base = f'http://127.0.0.1:{lab.server_port}'
    endpoint = '/preview/reflected-xss/' + mode
    packets = []
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_POST(self):
            packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            message = (call('run_local_lab', {'action':'run','run_id':'controlled-run','scenario':scenario,'url':base+endpoint}, 'lab-'+str(len(packets)))
                if len(packets) in (1,3) else {'role':'assistant','content':'Saved bounded marker observations; no execution or security conclusion.'})
            data = json.dumps({'choices':[{'message':message}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type','application/json')
            self.send_header('Content-Length',str(len(data)))
            self.end_headers()
            self.wfile.write(data)
    model = http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
    model_worker = threading.Thread(target=model.serve_forever,daemon=True)
    model_worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='blue-integrated-lab-') as tmp:
            root=Path(tmp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            cmd=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{model.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','3']
            level='read' if condition=='read_scope' else 'interact'
            consent='' if condition=='read_scope' else ('n\n' if condition=='denied' else 'y\n')
            first=subprocess.run(cmd+['chat'],env=env,cwd=workspace,text=True,capture_output=True,timeout=25,input=f'/scope allow {base}{endpoint} {level}\nRun the approved local marker scenario.\n{consent}/exit\n')
            assert first.returncode==0,first.stderr[-2000:]
            files=list((app/'sessions').glob('*.json'));assert len(files)==1
            session=json.loads(files[0].read_text())
            results=[json.loads(m['content']) for m in session['messages'] if m['role']=='tool']
            assert len(results)==1 and len(packets)==2
            result=results[0]
            expected_count=0 if condition in ('read_scope','denied') else 1 if condition in ('disconnect','truncated') else 3
            assert len(paths)==expected_count
            if condition=='read_scope':
                assert result['ok'] is False and not list((app/'sessions'/'lab-runs').glob('*/*.json'))
                return {'mode':mode,'condition':condition,'status':'passed','GETs':0,'scope_enforced':True}
            checkpoint=json.loads(next((app/'sessions'/'lab-runs').glob('*/*.json')).read_text())
            assert checkpoint['requests_attempted']==expected_count
            assert len(checkpoint['observations'])==len(wire_observations)
            for observed,wire in zip(checkpoint['observations'],wire_observations):
                assert all(observed[k]==v for k,v in wire.items()), (observed,wire)
            assert checkpoint['browser_execution_observed'] is False and checkpoint['vulnerability_confirmed'] is False
            expected_state={'disconnect':'request_outcome_unknown','truncated':'stopped_unusable_response','denied':'denied'}.get(condition,'completed')
            assert checkpoint['state']==expected_state
            if condition in ('normal','unstable','escape_removed','mixed_encoding'):
                expected=('inconclusive_control_changed' if condition=='unstable' else 'inconclusive_mixed_or_missing_encoding' if condition=='mixed_encoding' else
                    ('raw_probe_sequence_observed' if mode=='vulnerable' else 'escaped_probe_sequence_observed') if scenario=='inert_html' else 'differential_literal_marker_only')
                if checkpoint['verdict']!=expected and condition=='escape_removed':
                    assert scenario=='inert_html' and mode=='fixed'
                    assert checkpoint['verdict']=='raw_probe_sequence_observed'
                    assert all(o['probe_tags']==1 and not o['escaped_probe_text'] for o in parsed_oracles)
                    raise EncodingMismatch('Fixed-case regression detected: raw sequence plus independent parsed custom element')
                assert checkpoint['verdict']==expected
                assert len(checkpoint['observations'])==3
                if scenario=='inert_html':
                    assert all(o['probe_tags']==(1 if mode=='vulnerable' or condition=='mixed_encoding' else 0) for o in parsed_oracles)
                    assert all(o['escaped_probe_text']==(mode=='fixed') for o in parsed_oracles)
            # New process, same saved session and same run action: never dispatch again.
            second=subprocess.run(cmd+['--resume',session['id'],'chat'],env=env,cwd=workspace,text=True,capture_output=True,timeout=15,input='Inspect the same run without repeating it.\n/exit\n')
            assert second.returncode==0,second.stderr[-2000:]
            resumed=json.loads(files[0].read_text())
            saved=[json.loads(m['content']) for m in resumed['messages'] if m['role']=='tool'][-1]
            assert saved['fresh_evidence'] is False and saved['reused_local_copy'] is True
            assert saved['network_requests_this_call']==0 and len(paths)==expected_count
            assert saved['state']==expected_state and len(packets)==4
            # Export in a third process after scope revocation, without model or network.
            exported=subprocess.run(cmd+['--resume',session['id'],'chat'],env=env,cwd=workspace,text=True,capture_output=True,timeout=15,input='/scope clear\n/audit-report\n/exit\n')
            assert exported.returncode==0
            report=lab_report_from_stdout(exported.stdout);assert len(report['runs'])==1,report
            run=report['runs'][0]
            assert run['current_interact_scope_authorized'] is False
            assert run['observations']==checkpoint['observations']
            assert run['canonical_recipe']['inputs']==checkpoint['recipe']['inputs']
            expected_report=checkpoint.get('verdict') if expected_state=='completed' else 'not_executed_denied' if expected_state=='denied' else 'inconclusive_partial_or_unknown'
            assert run['recomputed_observation']==expected_report,(run,checkpoint)
            assert len(paths)==expected_count and len(packets)==4
            altered=json.loads(files[0].read_text())
            for message in altered['messages']:
                if message['role']!='tool':continue
                value=json.loads(message['content'])
                if value.get('run_id')=='controlled-run' and not value.get('reused_local_copy'):
                    value['requests_attempted']=99;message['content']=json.dumps(value)
            files[0].write_text(json.dumps(altered))
            checked=subprocess.run(cmd+['--resume',session['id'],'chat'],env=env,cwd=workspace,text=True,capture_output=True,timeout=15,input='/audit-report\n/exit\n')
            assert checked.returncode==0
            invalid=lab_report_from_stdout(checked.stdout);assert invalid['runs']==[] and invalid['rejected_evidence']>=1,invalid
            assert len(paths)==expected_count and len(packets)==4
            return {'status':'passed','scenario':scenario,'independent_HTML_parser':parsed_oracles,'mode':mode,'condition':condition,'GETs':len(paths),'state':checkpoint['state'],'verdict':checkpoint.get('verdict'),'observations':checkpoint['observations'],'independent_server_response_oracle_matched':True,'restarted_process_GETs':0,'report_exports':2,'report_GETs':0,'report_model_calls':0,'report_tamper_rejection_verified':True,'browser_execution_observed':False,'vulnerability_confirmed':False,'evidence_id':result['evidence_id']}
    finally:
        lab.shutdown();lab.server_close();worker.join(timeout=3)
        model.shutdown();model.server_close();model_worker.join(timeout=3)


def main(binary,lab):
    assert subprocess.check_output(['git','-C',str(lab),'rev-parse','HEAD'],text=True).strip()==LAB_COMMIT
    subprocess.run(['git','-C',str(lab),'diff','--quiet','HEAD','--'],check=True)
    sys.dont_write_bytecode=True
    sys.addaudithook(loopback_only)
    sys.path.insert(0,str(lab))
    from app import make_server
    rows=[case(binary,make_server,mode,condition) for mode,condition in [('vulnerable','normal'),('fixed','normal'),('fixed','unstable'),('fixed','disconnect'),('fixed','truncated'),('fixed','read_scope'),('fixed','denied')]]
    rows.extend(case(binary,make_server,mode,condition,'inert_html') for mode,condition in [('vulnerable','normal'),('fixed','normal'),('fixed','unstable'),('fixed','mixed_encoding')])
    try:
        case(binary,make_server,'fixed','escape_removed','inert_html')
    except EncodingMismatch:
        sensitivity={'status':'expected_regression_detected','fixture':'response_adapter_removed_fixed_encoding','GETs':3}
    else:
        raise AssertionError('Sensitivity control failed to detect removed encoding')
    print(json.dumps({'status':'passed','sensitivity':sensitivity,'total_lab_GETs_including_sensitivity':sum(r['GETs'] for r in rows)+3,'lab_commit':LAB_COMMIT,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'rows':rows,'lab_GETs':sum(r['GETs'] for r in rows),'limits':'Actual CLI and pinned lab HTTP; test-only adapters for unstable/truncated/disconnected responses. No browser or live provider. Python socket audit does not sandbox the Rust process.'}))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--lab',type=Path,required=True);a=p.parse_args();main(a.binary.resolve(),a.lab.resolve())
