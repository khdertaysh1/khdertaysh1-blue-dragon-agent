#!/usr/bin/env python3
"""Aggregate metadata only; reported usage, estimates and reservations stay separate."""
import argparse,collections,json
from pathlib import Path

def summarize(events):
    latest={}; unkeyed=[]
    for event in events:
        if event.get('dispatch_id'):latest[event['dispatch_id']]=event
        else:unkeyed.append(event)
    events=list(latest.values())+unkeyed
    dispatched=sorted((e for e in events if e.get('attempts',0)>0),key=lambda e:e['unix_ms'])
    peak_rpm=peak_reserved=peak_reported=0;left=0
    actual_events=0
    for right,e in enumerate(dispatched):
        while dispatched[left]['unix_ms']<=e['unix_ms']-60000:left+=1
        window=dispatched[left:right+1]
        reserved=sum(x.get('estimated_message_tokens',0)+x.get('estimated_tool_tokens',0)+(x.get('max_output_tokens') or 0) for x in window)
        reported=sum((x.get('usage') or {}).get('total_tokens', (x.get('usage') or {}).get('input_tokens',(x.get('usage') or {}).get('prompt_tokens',0))+(x.get('usage') or {}).get('output_tokens',(x.get('usage') or {}).get('completion_tokens',0))) for x in window)
        peak_rpm=max(peak_rpm,len(window));peak_reserved=max(peak_reserved,reserved);peak_reported=max(peak_reported,reported)
        actual_events+=bool(e.get('usage_reported'))
    return {'dispatches':len(dispatched),'blocked_before_dispatch':sum(e.get('attempts')==0 for e in events),'http_429':sum(e.get('http_status')==429 for e in dispatched),'peak_rolling_60s_requests':peak_rpm,'peak_rolling_60s_estimated_input_plus_max_output':peak_reserved,'peak_rolling_60s_reported_tokens':peak_reported if actual_events else None,'usage_reported_requests':actual_events,'usage_unreported_requests':len(dispatched)-actual_events,'by_kind':dict(collections.Counter(e.get('kind','unknown') for e in dispatched)),'total_request_elapsed_ms':sum(e.get('elapsed_ms',0) for e in dispatched),'limits':'Client dispatches, not server receipts. Input estimates are not tokenizer measurements; max output is a conservative reservation. Missing usage is unknown. Scope of an unclassified 429 remains unknown.'}
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);a=p.parse_args();files=list(a.directory.glob('requests-*.jsonl'))
    if not files:p.error('No audit records; set BLUE_DRAGON_PROVIDER_AUDIT_DIR before running')
    events=[json.loads(line) for f in files for line in f.read_text().splitlines()]
    print(json.dumps(summarize(events),indent=2))
