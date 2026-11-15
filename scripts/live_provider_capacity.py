#!/usr/bin/env python3
"""Explicit opt-in live anonymous Free test: synthetic fixture only; stop on failure."""
import argparse,hashlib,json,os,shutil,subprocess,time
from verify_live_provider_capacity import check_read
from pathlib import Path

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--allow-network',action='store_true');p.add_argument('--rounds',type=int,default=4);a=p.parse_args()
    if not a.allow_network:p.error('Network opt-in required')
    if not 1<=a.rounds<=30:p.error('rounds must be 1..30')
    a.output.mkdir(parents=True,exist_ok=False);root=a.output.resolve();work=root/'fixture';work.mkdir()
    code='''export function vulnerableRead(user, record) { return record.value; }
export function protectedRead(user, record) { if (user.id !== record.owner) throw new Error("denied"); return record.value; }
'''
    (work/'app.js').write_text(code+'// synthetic padding\n'*5000)
    fixture_hash=hashlib.sha256((work/'app.js').read_bytes()).hexdigest()
    env={k:v for k,v in os.environ.items() if k in ['PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy','SSL_CERT_FILE','SSL_CERT_DIR']};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),BLUE_DRAGON_PROVIDER_AUDIT_DIR=str(root/'audit'),BLUE_DRAGON_INPUT_BUDGET='8192',BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048',NO_COLOR='1',TERM='dumb')
    binary=root/('trial-'+a.binary.name);shutil.copy2(a.binary.resolve(),binary);binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest();previous_observations=0;cmd=[str(binary),'--provider','free','--root',str(work),'--read-only','--no-stream','--no-rotation','--max-steps','6'];rows=[]
    for i in range(a.rounds):
        if i:time.sleep(20)
        prompt='Read only bounded windows of app.js. NEVER MODIFY FILES. Compare vulnerableRead and protectedRead for a user whose id differs from record.owner. Cite source evidence and identify the missing authorization check. This is synthetic local code; distinguish static reasoning from runtime proof. Do not claim an actual deployed exploit.'
        if i:prompt='Continue our analysis, keeping earlier constraints. Recheck a bounded current source window, compare the two functions and explain one limitation of the evidence. Do not repeat a failed or uncertain action.'
        started=time.monotonic()
        try:
            result=subprocess.run(cmd+(['--resume','latest'] if i else [])+['run',prompt],text=True,capture_output=True,env=env,cwd=work,timeout=150)
        except subprocess.TimeoutExpired:
            (root/'interrupted.json').write_text(json.dumps({'round':i,'outcome':'unknown','reason':'CLI exceeded 150 seconds; no retry','completed_rounds':rows},indent=2))
            print('Live run timed out; outcome unknown; stopped without retry')
            return
        (root/f'round-{i}.stdout').write_text(result.stdout);(root/f'round-{i}.stderr').write_text(result.stderr)
        session=json.loads(next((root/'state'/'sessions').glob('*.json')).read_text());report=session.get('last_report',{});assistant=[m.get('content','') for m in session['messages'] if m['role']=='assistant' and not m.get('tool_calls')]
        observations=session.get('observations',[])
        reads=[]
        for observation in observations[previous_observations:]:
            if observation['tool'] not in ('read_file','read_file_window'):continue
            native=json.loads(session['messages'][observation['message_index']]['content'])
            reads.append(check_read(native,(work/'app.js').read_bytes(),observation_ok=observation.get('ok') is True) and observation['tool']=='read_file_window')
        fresh=bool(reads) and all(reads)
        previous_observations=len(observations)
        rows.append({'round':i,'fresh_source_recheck_verified':fresh,'binary_sha256':binary_hash,'exit_code':result.returncode,'elapsed_seconds':round(time.monotonic()-started,3),'finished':report.get('finished'),'native_observations_total':len(session.get('observations',[])),'last_request_metrics':report.get('context',{}).get('provider_request_metrics'),'provider_failure':report.get('provider_failure'),'partial':report.get('provider_partial'),'last_assistant_text':assistant[-1] if assistant else None})
        output={'binary_sha256':binary_hash,'provider':'Kilo anonymous, documented endpoint','requested_model':'kilo-auto/free','requested_rounds':a.rounds,'rounds':rows,'fixture_unchanged':hashlib.sha256((work/'app.js').read_bytes()).hexdigest()==fixture_hash,'limits':'Live observation from this host and time only. Short bounded trial, not proof of sustained capacity or another IP/account quota. Synthetic fixture only; no private project or credentials.'}
        (root/'result.json').write_text(json.dumps(output,indent=2)+'\n')
        if result.returncode or not report.get('finished') or not fresh:break
    print(json.dumps({'rounds_executed':len(rows),'finished_rounds':sum(bool(r['finished']) for r in rows),'fresh_verified_rounds':sum(bool(r['fresh_source_recheck_verified']) for r in rows),'result':str(root/'result.json')}))
if __name__=='__main__':main()
