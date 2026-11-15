#!/usr/bin/env python3
"""Opt-in anonymous free-provider trial. Approves only an exact loopback curl HEAD command."""
import argparse,hashlib,http.server,json,os,select,subprocess,sys,threading,time
from pathlib import Path

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--allow-network',action='store_true');a=p.parse_args()
 if not a.allow_network:p.error('Explicit network opt-in required')
 a.output.mkdir(parents=True,exist_ok=False);root=a.output.resolve();work=root/'project';work.mkdir();binary=a.binary.resolve();digest=hashlib.sha256(binary.read_bytes()).hexdigest()
 env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy','SSL_CERT_FILE','SSL_CERT_DIR')}
 env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),BLUE_DRAGON_PROVIDER_AUDIT_DIR=str(root/'audit'),NO_COLOR='1',TERM='dumb')
 hits=[]
 class Site(http.server.BaseHTTPRequestHandler):
  def log_message(self,*_):pass
  def do_HEAD(self):
   hits.append(self.path);self.send_response(200);self.send_header('X-Blue-Fixture','live-curl-check');self.end_headers()
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Site);threading.Thread(target=server.serve_forever,daemon=True).start()
 expected_args=['-sS','-I','--connect-timeout','10','--max-time','20',f'http://127.0.0.1:{server.server_port}/head']
 prompt='Use run_command (discover it if necessary) to execute exactly this payload after approval: '+json.dumps({'program':'curl','args':expected_args,'timeout_seconds':30})+'. Fetch only these local response headers. Do not use browser, shell, install, modify files or contact any other website. Report the native status line, X-Blue-Fixture header and exit_code. Do not claim success without executing.'

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
   permitted=approved==0 and preview.get('program')=='curl' and preview.get('args')==expected_args and preview.get('timeout_seconds')==30 and preview.get('shell_expansion') is False
   child.stdin.write(b'y\n' if permitted else b'n\n');child.stdin.flush();handled=markers;approved+=int(permitted);decisions.append({'approved':permitted,'reason':'exact loopback curl HEAD allowlist' if permitted else 'outside trial allowlist'})
 try:child.wait(timeout=5)
 except subprocess.TimeoutExpired:child.kill();child.wait()
 capture.extend(child.stdout.read() or b'');(root/'console.txt').write_bytes(capture)
 sessions=list((root/'state/sessions').glob('*.json'));results=[];finished=False
 if sessions:
  session=json.loads(sessions[0].read_text());finished=session.get('last_report',{}).get('finished',False)
  for observation in session.get('observations',[]):
   if observation['tool']=='run_command':results.append(json.loads(session['messages'][observation['message_index']]['content']))
 successful=[r for r in results if r.get('ok') is True and 'X-Blue-Fixture: live-curl-check' in r.get('stdout','') and r.get('stdout','').split()[:2]==['HTTP/1.0','200'] and r.get('exit_code')==0 and r.get('execution_status')=='completed' and not r.get('cached_result')]
 report={'binary_sha256':digest,'binary_unchanged':hashlib.sha256(binary.read_bytes()).hexdigest()==digest,'exit_code':child.returncode,'timed_out_outcome_unknown':timed_out,'approval_decisions':decisions,'native_command_results':results,'native_curl_verified':len(successful)==1 and approved==1 and hits==['/head'],'loopback_requests':hits,'finished':finished,'elapsed_seconds':round(time.monotonic()-started,3),'limits':'One live-model curl HEAD task against a controlled loopback site on Linux, not physical Termux, long-session capacity or proof that a live model emitted malformed types. No credentials or paid fallback.'}
 server.shutdown();server.server_close()
 (root/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='native_command_results'}))
if __name__=='__main__':main()
