"""Synthetic stdio resources only; no network, file reads or external credentials."""
import argparse
import json
import sys
p=argparse.ArgumentParser();p.add_argument('--mode',default='normal');p.add_argument('--log');a=p.parse_args()
initialized=False
for line in sys.stdin:
    req=json.loads(line);method=req.get('method')
    if a.log:
        with open(a.log,'a') as log: log.write(method+'\n')
    if method=='notifications/initialized':initialized=True;continue
    if method=='initialize':result={'protocolVersion':'2025-03-26','capabilities':{'resources':{}}}
    elif method=='resources/list':
        assert initialized
        if req.get('params',{}).get('cursor')=='page-2':result={'resources':[{'uri':'fixture://notes','name':'Notes'}]}
        else:result={'resources':[{'uri':'fixture://schema','name':'Schema','description':'Synthetic schema context'}],'nextCursor':'page-2'}
    elif method=='resources/read':
        assert initialized
        if a.mode=='disconnect':sys.exit(0)
        uri=req['params']['uri'];assert uri in ('fixture://schema','fixture://notes')
        result={'contents':[{'uri':'fixture://unexpected' if a.mode=='mismatch' else uri,'mimeType':'text/plain','text':'PRIVATE_CONTEXT_'*10000 if a.mode=='oversize' else 'SCHEMA_FIXTURE' if uri.endswith('schema') else 'NOTE_FIXTURE'}]}
    else:raise AssertionError(method)
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}),flush=True)
