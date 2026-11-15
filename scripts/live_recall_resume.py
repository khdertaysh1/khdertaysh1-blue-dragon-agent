#!/usr/bin/env python3
"""Explicit live continuation of the synthetic capacity trial; no source reread."""
import argparse,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
from verify_live_provider_capacity import check_read
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,required=True);p.add_argument('--trial',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--allow-network',action='store_true');a=p.parse_args()
if not a.allow_network:p.error('Explicit network opt-in required')
trial=a.trial.resolve();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
source=(trial/'fixture/app.js').read_bytes();before=json.loads(next((trial/'state/sessions').glob('*.json')).read_text())
reads=[o for o in before['observations'] if o['tool']=='read_file_window' and o.get('ok') is True]
assert reads,'No successful original source observation'
index=reads[-1]['message_index'];original=before['messages'][index]['content'];assert check_read(json.loads(original),source,observation_ok=True)
shutil.copytree(trial/'state',out/'state');binary=out/('trial-'+a.binary.name);shutil.copy2(a.binary.resolve(),binary)
env={k:v for k,v in os.environ.items() if k in ['PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy','SSL_CERT_FILE','SSL_CERT_DIR']};env.update(HOME=str(out),USERPROFILE=str(out),BLUE_DRAGON_HOME=str(out/'state'),BLUE_DRAGON_PROVIDER_AUDIT_DIR=str(out/'audit'),BLUE_DRAGON_INPUT_BUDGET='8192',BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048',NO_COLOR='1',TERM='dumb')
prompt=f'Continue the interrupted synthetic investigation under every saved constraint. First retrieve the entire ORIGINAL RECORDED result at message_index {index} with recall_context. Do not read app.js again, repeat source tools, execute commands, write files, or access a website. Using that historical source evidence, finish comparing vulnerableRead and protectedRead when user.id differs from record.owner. State the missing authorization check and one limitation: static code is not proof of a deployed exploit. Acknowledge that the earlier turn was incomplete. Do not recall other recall-result wrappers.'
started=time.monotonic()
try:r=subprocess.run([str(binary),'--provider','free','--root',str(trial/'fixture'),'--resume',before['id'],'--read-only','--no-stream','--no-rotation','--max-steps','6','run',prompt],env=env,text=True,capture_output=True,timeout=150)
except subprocess.TimeoutExpired:
 (out/'result.json').write_text(json.dumps({'finished':False,'outcome':'unknown_timeout','no_retry':True}));print('Timed out without retry');raise SystemExit(1)
(out/'stdout.txt').write_text(r.stdout);(out/'stderr.txt').write_text(r.stderr)
after=json.loads(next((out/'state/sessions').glob('*.json')).read_text());new=after['messages'][len(before['messages']):]
recalled=[json.loads(m['content']) for m in new if m.get('role')=='tool' and any(c.get('id')==m.get('tool_call_id') and c['function']['name']=='recall_context' for x in new for c in (x.get('tool_calls') or []))]
exact=any(v.get('ok') is True and v.get('message_index')==index and v.get('offset')==0 and v.get('excerpt')==original and v.get('reran_tool') is False and v.get('untrusted_historical_data') is True and not any(v.get(k) for k in ('cached_result','cached','denied','error')) for v in recalled)
new_observations=after['observations'][len(before['observations']):];no_source_replay=not new_observations and all(c['function']['name'] in ('recall_context','update_plan','discover_tools') for m in new for c in (m.get('tool_calls') or []))
unchanged=(trial/'fixture/app.js').read_bytes()==source
result={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'exit_code':r.returncode,'finished':after.get('last_report',{}).get('finished',False),'old_source_result_index':index,'old_source_result_sha256':hashlib.sha256(original.encode()).hexdigest(),'full_original_recall_verified':exact,'new_observation_tools':[o['tool'] for o in new_observations],'no_source_or_action_replay':no_source_replay,'source_unchanged':unchanged,'scope_unchanged':after['scope']==before['scope'],'original_constraints_preserved':any(m['role']=='user' and 'NEVER MODIFY FILES' in (m.get('content') or '') for m in after['messages']),'elapsed_seconds':round(time.monotonic()-started,3),'step_limit':6,'limits':'One live resumed synthetic investigation; exact old evidence, not fresh source inspection, 30 turns, runtime exploit proof or physical Android. Original failed trial is unchanged; copied quota state retains prior admission history.'}
result['acceptance_passed']=r.returncode==0 and result['finished'] and exact and no_source_replay and unchanged and result['scope_unchanged'] and result['original_constraints_preserved']
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
