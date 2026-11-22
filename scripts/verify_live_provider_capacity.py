#!/usr/bin/env python3
"""Independently check saved synthetic live trials; no provider/network calls."""
import argparse, hashlib, json
from pathlib import Path


def check_read(result, source):
    if result.get('ok') is False or result.get('denied') or result.get('execution_status') in ('unknown','stale'):
        return False
    if 'byte_offset' in result:
        a,b=result.get('byte_offset'),result.get('end_byte_offset')
        if not isinstance(a,int) or not isinstance(b,int) or not 0<=a<=b<=len(source):return False
        expected=source[a:b]
        return result.get('content')==expected.decode('utf-8') and result.get('window_sha256')==hashlib.sha256(expected).hexdigest()
    return result.get('content')==source.decode('utf-8') and result.get('sha256')==hashlib.sha256(source).hexdigest()


def verify(root):
    trial=json.loads((root/'result.json').read_text());session=json.loads(next((root/'state/sessions').glob('*.json')).read_text());source=(root/'fixture/app.js').read_bytes()
    observations=session.get('observations',[]);previous=0;rounds=[];full_reads=0
    for row in trial['rounds']:
        stop=row['native_observations_total'];reads=[]
        for observation in observations[previous:stop]:
            if observation['tool'] not in ('read_file','read_file_window'):continue
            result=json.loads(session['messages'][observation['message_index']]['content']);valid=check_read(result,source)
            reads.append({'tool':observation['tool'],'evidence_id':observation['id'],'native_source_match':valid})
            full_reads+=observation['tool']=='read_file'
        text=row.get('last_assistant_text') or ''
        rounds.append({'round':row['round'],'cli_finished':row['finished'],'new_native_reads':reads,'fresh_source_recheck_verified':any(r['native_source_match'] for r in reads),'comparison_terms_present':all(term in text for term in ('vulnerableRead','protectedRead','owner'))})
        previous=stop
    assert trial['fixture_unchanged']
    assert any(m['role']=='user' and 'NEVER MODIFY FILES' in (m.get('content') or '') for m in session['messages'])
    assert all(r['native_source_match'] for row in rounds for r in row['new_native_reads'])
    # Adversarial checks: matching text alone, denied output or forged hash is not evidence.
    good={'content':source.decode(),'sha256':hashlib.sha256(source).hexdigest()}
    assert check_read(good,source)
    assert not check_read(dict(good,denied=True),source)
    assert not check_read(dict(good,sha256='0'*64),source)
    assert not check_read(dict(good,execution_status='unknown'),source)
    return {'rounds':rounds,'full_file_reads_despite_window_only_prompt':full_reads,'original_constraint_retained':True,'native_results_verified':True,'limits':'A finished CLI round is not semantic success. Fresh reads are checked against disk/hash independently. Comparison-term checks are not a reasoning-quality oracle; narrative conclusions require review. Full-file reads violate the window-only test instruction even if later provider projection is bounded.'}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);a=p.parse_args();print(json.dumps(verify(a.directory),indent=2))
