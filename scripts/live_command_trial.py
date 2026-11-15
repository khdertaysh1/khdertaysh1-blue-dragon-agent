#!/usr/bin/env python3
"""Opt-in anonymous free-provider trial. Approves only an exact local print command."""
import argparse,hashlib,json,os,select,subprocess,sys,time
from pathlib import Path

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--allow-network',action='store_true');a=p.parse_args()
 if not a.allow_network:p.error('Explicit network opt-in required')
 a.output.mkdir(parents=True,exist_ok=False);root=a.output.resolve();work=root/'project';work.mkdir();binary=a.binary.resolve();digest=hashlib.sha256(binary.read_bytes()).hexdigest()
 env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy','SSL_CERT_FILE','SSL_CERT_DIR')}
 env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),BLUE_DRAGON_PROVIDER_AUDIT_DIR=str(root/'audit'),NO_COLOR='1',TERM='dumb')
 prompt="Use run_command (discover it if necessary) to run exactly program python3 with args [\"-c\", \"print('BLUE_COMMAND_LIVE')\"] and timeout_seconds 30. Do not modify files, install, use shell, or contact websites. Report only the native stdout and exit_code; do not claim success without executing."
 child=subprocess.Popen([str(binary),'--root',str(work),'--provider','free','--no-stream','--no-rotation','--max-steps','5','run',prompt],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env,bufsize=0)
 capture=bytearray();handled=0;approved=0;decisions=[];started=time.monotonic();timed_out=False
 while child.poll() is None:
  if time.monotonic()-started>120:timed_out=True;child.terminate();break
  if select.select([child.stdout],[],[],0.1)[0]:
   data=os.read(child.stdout.fileno(),65536)
   if data:capture.extend(data)
  text=capture.decode(errors='replace');markers=text.count('Approve this ONE action? [y/N]')
  if markers>handled:
   block=text.rsplit('┌─ APPROVAL REQUIRED',1)[-1].split('└',1)[0]
   lines=[line[2:] for line in block.splitlines() if line.startswith('│ ')]
   try:preview=json.loads('\n'.join(lines[1:]))
   except ValueError:preview={}
   permitted=approved==0 and preview.get('program') in ('python3',sys.executable) and preview.get('args')==['-c',"print('BLUE_COMMAND_LIVE')"] and preview.get('timeout_seconds')==30 and preview.get('shell_expansion') is False
   child.stdin.write(b'y\n' if permitted else b'n\n');child.stdin.flush();handled=markers;approved+=int(permitted);decisions.append({'approved':permitted,'reason':'exact local print allowlist' if permitted else 'outside trial allowlist'})
 try:child.wait(timeout=5)
 except subprocess.TimeoutExpired:child.kill();child.wait()
 capture.extend(child.stdout.read() or b'');(root/'console.txt').write_bytes(capture)
 sessions=list((root/'state/sessions').glob('*.json'));results=[];finished=False
 if sessions:
  session=json.loads(sessions[0].read_text());finished=session.get('last_report',{}).get('finished',False)
  for observation in session.get('observations',[]):
   if observation['tool']=='run_command':results.append(json.loads(session['messages'][observation['message_index']]['content']))
 successful=[r for r in results if r.get('ok') is True and r.get('stdout')=='BLUE_COMMAND_LIVE\n' and r.get('exit_code')==0]
 report={'binary_sha256':digest,'binary_unchanged':hashlib.sha256(binary.read_bytes()).hexdigest()==digest,'exit_code':child.returncode,'timed_out_outcome_unknown':timed_out,'approval_decisions':decisions,'native_command_results':results,'native_print_verified':len(successful)==1 and approved==1,'finished':finished,'elapsed_seconds':round(time.monotonic()-started,3),'limits':'One synthetic local print task on Linux, not physical Termux, long-session capacity or proof that a live model emitted malformed types. No credentials or paid fallback.'}
 (root/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='native_command_results'}))
if __name__=='__main__':main()
