#!/usr/bin/env python3
"""Scripted CLI acceptance: prose and prior reads cannot satisfy a fresh recheck."""
import argparse, hashlib, http.server, json, os, subprocess, tempfile, threading
from pathlib import Path


def run(binary, mode, expect_regression=False):
    packets=[]
    with tempfile.TemporaryDirectory(prefix='blue-fresh-') as directory:
        root=Path(directory);work=root/'project';work.mkdir()
        source=work/'app.js';source.write_text('export function protectedRead(user, record) { if(user.id !== record.owner) throw Error("denied"); return record.value; }\n'+'// padding\n'*10000)
        original=source.read_bytes();(work/'other.js').write_bytes(original)
        class Model(http.server.BaseHTTPRequestHandler):
            def log_message(self,*_):pass
            def do_POST(self):
                packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));index=len(packets);packets.append(packet)
                do_read=(mode in ('range_wrong','range_correct','range_resume') and index==0) or (mode=='range_resume' and index==3) or (mode=='resume_recover' and index==2) or (mode=='recover' and index==1) or (mode in ('wrong_path','stale','failed','old') and index==0)
                if do_read:
                    path='other.js' if mode=='wrong_path' else '../outside.js' if mode=='failed' else 'app.js'
                    message={'role':'assistant','content':'','tool_calls':[{'id':f'call-{index}','type':'function','function':{'name':'read_file_window','arguments':json.dumps({'path':path,'byte_offset':1000 if mode=='range_correct' or (mode=='range_resume' and index==3) else 0,'byte_count':100 if mode=='range_correct' or (mode=='range_resume' and index==3) else 512})}}]}
                else:
                    if mode=='stale' and index==1:source.write_bytes(original.replace(b'protectedRead',b'changedReader',1))
                    message={'role':'assistant','content':'I checked the source; this text alone must never be accepted as fresh evidence.'}
                data=json.dumps({'choices':[{'message':message,'finish_reason':'tool_calls' if do_read else 'stop'}]}).encode()
                self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
        env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT')};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb')
        command=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--read-only','--max-steps','6']
        try:
            prompt='Read bounded windows of ../outside.js again.' if mode=='failed' else 'Read bounded windows of app.js again.'
            if mode.startswith('range_'):prompt='Read bounded window of app.js bytes 1000..1100 again.'
            if mode=='range_unresolved':prompt='Read bytes 1000..1100 again.'
            process=subprocess.run(command+['run',prompt],env=env,text=True,capture_output=True,timeout=40)
            if mode in ('old','resume_recover','range_resume'):
                assert process.returncode==(0 if mode=='old' else 1),process.stderr
                process=subprocess.run(command+['--resume','latest','run','Recheck the current source window.' if mode=='old' else 'كمل'],env=env,text=True,capture_output=True,timeout=40)
            saved=json.loads(next((root/'state/sessions').glob('*.json')).read_text())
            if expect_regression:
                assert mode=='missing' and process.returncode==0 and saved['last_report']['finished'] is True
                assert not any(o['tool'] in ('read_file','read_file_window') for o in saved.get('observations',[]))
                assert len(packets)==1
                return {'mode':mode,'requests':len(packets),'exit_code':process.returncode,'regression_reproduced':'Final prose accepted without the explicitly requested native read'}
            check=saved['last_report']['fresh_source_checks']
            if mode in ('recover','resume_recover','range_correct','range_resume'):
                assert process.returncode==0 and len(packets)=={'recover':3,'resume_recover':4,'range_correct':2,'range_resume':5}[mode],(process.stderr,check)
                assert check['all_observed'] is True
                proof=check['checks'][0]['proof'];assert proof['window_sha256']==hashlib.sha256(original[1000:1100] if mode.startswith('range_') else original[:512]).hexdigest()
                assert proof['arguments']['path']=='app.js'
            else:
                assert process.returncode!=0 and check['all_observed'] is False,(mode,process.stderr,check)
                assert saved['last_report']['task_status']=='needs_verification'
                assert len(packets)=={'missing':2,'wrong_path':3,'stale':3,'failed':2,'old':4,'range_wrong':3,'range_unresolved':1}[mode],(mode,len(packets))
            assert source.read_bytes()==(original.replace(b'protectedRead',b'changedReader',1) if mode=='stale' else original)
            return {'mode':mode,'requests':len(packets),'exit_code':process.returncode,'states':[r['state'] for r in check['checks']],'passed':True}
        finally:server.shutdown();server.server_close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');a=p.parse_args()
    print(json.dumps({'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'cases':[run(a.binary.resolve(),mode,a.expect_regression) for mode in (('missing',) if a.expect_regression else ('missing','recover','resume_recover','wrong_path','stale','failed','old','range_wrong','range_correct','range_resume','range_unresolved'))],'limits':'Scripted provider, real CLI and native file reads. No live reasoning or quota claim.'},indent=2))
