#!/usr/bin/env python3
"""Sequential offline/loopback qualification of one immutable Blue Dragon binary.
No downloads, real browsers, live providers, remote repository writes or installs.
The reviewed pinned lab must already exist locally. Logs may contain fixture data.
"""
import argparse,hashlib,json,os,platform,shutil,signal,subprocess,sys,time
from pathlib import Path
from lab_matrix import LAB_COMMIT


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stop(process):
    if os.name=='posix':os.killpg(process.pid,signal.SIGTERM)
    else:process.terminate()
    try:process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        if os.name=='posix':os.killpg(process.pid,signal.SIGKILL)
        else:process.kill()
        process.wait(timeout=5)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--lab',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True,help='New directory; existing output is never overwritten')
    parser.add_argument('--timeout',type=int,default=120,help='Per-group seconds, 15..300')
    args=parser.parse_args()
    if not 15<=args.timeout<=300:parser.error('timeout must be 15..300')
    repo=Path(__file__).resolve().parent.parent;binary=args.binary.resolve();lab=args.lab.resolve();output=args.output.resolve()
    if not binary.is_file():parser.error('binary must be an existing regular file')
    commit=subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()
    if subprocess.check_output(['git','-C',str(lab),'rev-parse','HEAD'],text=True).strip()!=LAB_COMMIT:parser.error('lab commit mismatch')
    subprocess.run(['git','-C',str(lab),'diff','--quiet','HEAD','--'],check=True)
    output.mkdir(parents=True,exist_ok=False)
    env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT','LANG','LC_ALL'}}
    home=output/'isolated-home';home.mkdir()
    env.update(HOME=str(home),USERPROFILE=str(home),XDG_CONFIG_HOME=str(home/'config'),XDG_DATA_HOME=str(home/'data'),BLUE_DRAGON_BINARY=str(binary),PYTHONDONTWRITEBYTECODE='1',NO_COLOR='1')
    groups=[]
    for name in ['smoke_command_arguments','smoke_fresh_source','smoke_delegate_edits','smoke_provider_boundaries','smoke_continuity','smoke_coverage','smoke_test_coverage','smoke_delegates','smoke_delegate_consistency','smoke_integrated_research','smoke_mcp_resources','smoke_memory_sources','smoke_source_refresh','smoke_source_chain','smoke_review_engines']:
        groups.append((name,[sys.executable,str(repo/'scripts'/f'{name}.py'),str(binary)],True))
    for name in ['lab_matrix','smoke_lab_runner','smoke_source_discovery','smoke_dynamic_discovery','smoke_scoped_flow','smoke_call_bridge']:
        groups.append((name,[sys.executable,str(repo/'scripts'/f'{name}.py'),str(binary),'--lab',str(lab)],True))
    groups.append(('models_offline',[sys.executable,str(repo/'scripts/smoke_models.py')],True))
    for name in ['smoke_tui','smoke_job_shutdown','test_pty_shutdown','test_installer','test_source_upgrade','test_release_payload']:
        groups.append((name,[sys.executable,str(repo/'scripts'/f'{name}.py')],os.name=='posix'))
    for name in ['test_browser_forms','test_browser_policy']:
        groups.append((name,['node',str(repo/'scripts'/f'{name}.mjs')],shutil.which('node') is not None))
    groups.append(('comparison_negative_controls',[sys.executable,str(repo/'scripts/test_benchmark_alpha_reads.py')],True))
    groups.append(('common_read_blue_preflight',[sys.executable,str(repo/'scripts/benchmark_alpha_reads.py'),'--blue',str(binary),'--output',str(output/'common-read-blue.json')],True))
    groups.append(('workflow_oracle_controls',[sys.executable,str(repo/'scripts/test_benchmark_alpha_workflows.py')],True))
    groups.append(('edit_workflow_blue_preflight',[sys.executable,str(repo/'scripts/benchmark_alpha_workflows.py'),'--blue',str(binary),'--output',str(output/'edit-workflow-blue.json')],True))
    groups.append(('runtime_benchmark',[sys.executable,str(repo/'scripts/benchmark_runtime.py'),'--candidate',str(binary),'--repeats','3','--output',str(output/'runtime.json')],True))
    report={'schema':1,'source_commit':commit,'source_dirty_at_start':bool(subprocess.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip()),'platform':platform.platform(),'python':sys.version.split()[0],'lab_commit':LAB_COMMIT,'binary_sha256':sha(binary),'groups':[],
        'limits':['Synthetic/offline fixtures and reviewed loopback lab only; no OS network isolation claim.','No real browser, physical Termux, Windows/macOS execution or live provider qualification.','No Alpha runtime comparison; deterministic runtime tests do not measure model quality.','POSIX process groups are terminated on timeout; Windows only terminates the immediate process.','Group pass means its assertions and exit code passed; no count or coverage equivalence across groups.']}
    def save():
        temp=output/'summary.tmp';temp.write_text(json.dumps(report,indent=2)+'\n');temp.replace(output/'summary.json')
    save()
    for name,command,supported in groups:
        script=Path(command[1]);row={'name':name,'harness_sha256':sha(script),'status':'running','log':name+'.log'}
        report['groups'].append(row)
        if not supported:
            row.update(status='not_run',reason='Required runtime or platform unavailable');save();continue
        print('RUN '+name,flush=True);save();started=time.monotonic()
        with (output/row['log']).open('wb') as log:
            process=subprocess.Popen(command,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT,stdin=subprocess.DEVNULL,start_new_session=os.name=='posix')
            try:
                code=process.wait(timeout=args.timeout);row.update(status='passed' if code==0 else 'failed',exit_code=code)
            except subprocess.TimeoutExpired:
                stop(process);row.update(status='timed_out',timeout_seconds=args.timeout)
        row['seconds']=round(time.monotonic()-started,3);row['log_sha256']=sha(output/row['log']);save()
        print(row['status'].upper()+' '+name,flush=True)
    report['binary_unchanged']=sha(binary)==report['binary_sha256']
    report['status']='passed' if report['binary_unchanged'] and all(r['status']=='passed' for r in report['groups']) else 'incomplete_or_failed'
    save();print(json.dumps({'status':report['status'],'groups':len(groups),'summary':str(output/'summary.json')}))
    return 0 if report['status']=='passed' else 1

if __name__=='__main__':sys.exit(main())
