#!/usr/bin/env python3
"""Strict wrapper around the unchanged independent marked-prose review fixture."""
import hashlib,json,os,subprocess,sys
from pathlib import Path
binary=Path(sys.argv[1]).resolve()
fixture=Path(__file__).parent/'fixtures/review_fully_distinct.py'
env=dict(os.environ,REVIEW_BINARY=str(binary))
run=subprocess.run([sys.executable,str(fixture)],env=env,text=True,capture_output=True,timeout=110)
assert run.returncode==0,run.stderr
result=json.loads(run.stdout)
assert result['requested_turns']==30 and result['provider_requests']==30,result
assert result['context_error_mentions']==0,result
assert result['binary_sha256']==hashlib.sha256(binary.read_bytes()).hexdigest()
result['fixture_sha256']=hashlib.sha256(fixture.read_bytes()).hexdigest()
result['limits']='Unchanged independent scripted-provider fixture; no live-model reasoning, browser or phone claim.'
result.pop('tail',None)
print(json.dumps(result,indent=2))
