#!/usr/bin/env python3
"""Real CLI/model/delegate/scope/report integration with an authenticated MOCK browser peer.
No real browser/DOM is exercised; the peer deliberately disconnects after one dispatched action.
"""
import argparse, base64, hashlib, http.server, json, os, socket, struct, subprocess, tempfile, threading, time
from pathlib import Path
from urllib.parse import urlparse

CAPS='open navigate snapshot click fill select set_checked scroll evaluate network console source screenshot close'.split()


def receive(sock, size):
    data=b''
    while len(data)<size:
        chunk=sock.recv(size-len(data))
        if not chunk:raise EOFError('peer closed')
        data+=chunk
    return data


def send_frame(sock, value, opcode=1):
    body=json.dumps(value).encode() if opcode==1 else value
    mask=os.urandom(4);length=len(body)
    header=bytes([0x80|opcode,0x80|length]) if length<126 else bytes([0x80|opcode,0x80|126])+struct.pack('!H',length)
    sock.sendall(header+mask+bytes(b^mask[i%4] for i,b in enumerate(body)))


def read_frame(sock):
    a,b=receive(sock,2);assert a&0x80,'Fragmented fixture frame unsupported'
    size=b&127
    if size==126:size=struct.unpack('!H',receive(sock,2))[0]
    if size==127:size=struct.unpack('!Q',receive(sock,8))[0]
    assert size<=1024*1024,'Browser frame exceeds fixture budget'
    mask=receive(sock,4) if b&128 else None
    body=receive(sock,size)
    if mask:body=bytes(v^mask[i%4] for i,v in enumerate(body))
    return a&15,body


def connect_peer(pair, token):
    url=urlparse(pair['url']);sock=socket.create_connection((url.hostname,url.port),timeout=5);sock.settimeout(30)
    key=base64.b64encode(os.urandom(16)).decode()
    sock.sendall((f'GET /bridge HTTP/1.1\r\nHost: 127.0.0.1:{url.port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nOrigin: chrome-extension://synthetic-fixture\r\n\r\n').encode())
    header=b''
    while not header.endswith(b'\r\n\r\n'):
        header+=receive(sock,1);assert len(header)<8192
    assert b' 101 ' in header
    expected=base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest())
    assert expected in header
    send_frame(sock,{'type':'hello','version':1,'token':token,'capabilities':CAPS})
    return sock


class CLI:
    def __init__(self, command, env, workspace):
        self.output='';self.errors=''
        self.process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',env=env,cwd=workspace,bufsize=1)
        def read(stream,attr):
            while True:
                char=stream.read(1)
                if not char:break
                setattr(self,attr,getattr(self,attr)+char)
        for stream,attr in [(self.process.stdout,'output'),(self.process.stderr,'errors')]:threading.Thread(target=read,args=(stream,attr),daemon=True).start()
        self.wait_prompt(0)
    def wait_prompt(self,start):
        deadline=time.monotonic()+15
        while 'blue-dragon> ' not in self.output[start:]:
            assert self.process.poll() is None,self.errors
            if time.monotonic()>deadline:raise AssertionError('CLI prompt deadline: '+self.errors[-1000:])
            time.sleep(.01)
    def send(self,text,approvals=0):
        start=len(self.output);answered=0;self.process.stdin.write(text+'\n');self.process.stdin.flush();deadline=time.monotonic()+40
        while 'blue-dragon> ' not in self.output[start:]:
            needed=self.output[start:].count('Approve this ONE action? [y/N] ')
            if needed>answered:
                assert answered<approvals,'Unexpected approval prompt: '+self.output[start:][-1000:]
                self.process.stdin.write('y\n');self.process.stdin.flush();answered+=1
            assert self.process.poll() is None,self.errors
            if time.monotonic()>deadline:raise AssertionError('Command deadline: '+self.errors[-1000:])
            time.sleep(.01)
        assert answered==approvals,(text,answered,approvals)
        return self.output[start:]
    def close(self):
        if self.process.poll() is None:
            self.process.stdin.write('/exit\n');self.process.stdin.flush();self.process.wait(timeout=10)
        assert self.process.returncode==0,self.errors


def objects(text):
    decoder=json.JSONDecoder();values=[]
    for index,char in enumerate(text):
        if char!='{':continue
        try:value,_=decoder.raw_decode(text[index:])
        except ValueError:continue
        if isinstance(value,dict):values.append(value)
    return values


def main(binary):
    state={'phase':'research','parent':0,'children':{},'packets':0,'gets':0,'bridge':[],'errors':[]}
    children_started=threading.Event();browser_opened=threading.Event();held=set();held_lock=threading.Lock()
    def tool(name,args,ident):return {'role':'assistant','tool_calls':[{'id':ident,'type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}
    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_GET(self):
            state['gets']+=1;body=b'SYNTHETIC_HTTP_EVIDENCE';self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_POST(self):
            try:
                size=int(self.headers['Content-Length']);assert size<=2*1024*1024
                packet=json.loads(self.rfile.read(size));state['packets']+=1
                assert pair_token not in json.dumps(packet),'Pairing token leaked to model'
                messages=packet['messages'];child='independent read-only research delegate' in str(messages[0]['content'])
                results=[json.loads(m['content']) for m in messages if m['role']=='tool']
                if child:
                    who='a' if 'TASK_A' in str(messages[1]['content']) else 'b';step=state['children'].get(who,0)+1;state['children'][who]=step
                    if step==1:
                        with held_lock:
                            held.add(who)
                            if len(held)==2:children_started.set()
                        assert browser_opened.wait(10),'Parent did not dispatch browser open while children were active'
                        message=tool('file_outline',{'path':who+'.py'},who+'-read')
                    elif step==2:
                        message=tool('browser',{'action':'open','url':site},who+'-browser')
                        message['tool_calls']+=tool('file_outline',{'path':('b' if who=='a' else 'a')+'.py'},who+'-outside')['tool_calls']
                    else:
                        assert results[-1]['denied'] is True and results[-2]['denied'] is True
                        message={'role':'assistant','content':'Child interpretation is not native proof.'}
                else:
                    state['parent']+=1;step=state['parent'];phase=state['phase'];message={'role':'assistant','content':'Synthetic phase finished; limits retained.'}
                    if phase=='research':
                        if step<=2:message=tool('delegate_agents',{'action':'spawn','task':f'TASK_{"A" if step==1 else "B"} inspect only assigned local source','paths':[('a' if step==1 else 'b')+'.py']},f'p-{phase}-{step}')
                        elif step==3:message=tool('browser',{'action':'open','url':site},'p-open')
                        elif step==4:message=tool('browser',{'action':'snapshot'},'p-snapshot')
                        elif step==5:message=tool('web_fetch',{'url':site},'p-fetch')
                        else:
                            reports=[r for r in results if 'agents' in r]
                            if not reports or not all(a['state']=='completed' for a in reports[-1]['agents']):message=tool('delegate_agents',{'action':'wait','timeout_ms':500},f'p-wait-{step}')
                            elif not any('delegate_evidence' in r for r in results):message=tool('task_continuity',{},'p-continuity')
                    elif phase=='revoked':
                        if step==1:message=tool('web_fetch',{'url':site},'p-revoked-fetch')
                        elif step==2:message=tool('browser',{'action':'navigate','url':site},'p-revoked-browser')
                    elif phase=='disconnect':
                        if step==1:message=tool('browser',{'action':'click','selector':'#submit'},'p-unknown-click')
                        elif step==2:message=tool('task_continuity',{},'p-after-disconnect')
                body=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            except Exception as exc:state['errors'].append(str(exc));self.send_error(500)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=server.serve_forever,daemon=True).start();site=f'http://127.0.0.1:{server.server_port}/fixture';pair_token='UNSET_PAIR_TOKEN'
    cli=None;peer=None
    try:
        with tempfile.TemporaryDirectory(prefix='blue-integrated-') as temp:
            root=Path(temp);workspace=root/'workspace';workspace.mkdir();app=root/'state'
            for name in ['a','b']:(workspace/(name+'.py')).write_text(f'def {name}_source():\n    return 1\n')
            env={k:v for k,v in os.environ.items() if k.upper() in {'PATH','SYSTEMROOT','WINDIR','TMP','TEMP','COMSPEC','PATHEXT'}}
            env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(app),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),TERM='dumb',NO_COLOR='1')
            base=[str(binary),'--root',str(workspace),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--max-steps','24']
            cli=CLI(base+['chat'],env,workspace)
            pair=next(v for v in objects(cli.send('/browser start 0')) if 'pairing_token' in v);pair_token=pair['pairing_token']
            bad=connect_peer(pair,'0'*64)
            try:
                opcode,_=read_frame(bad);assert opcode==8,'Invalid token was accepted'
            except EOFError:pass
            finally:bad.close()
            peer=connect_peer(pair,pair_token);opcode,body=read_frame(peer);assert opcode==1 and json.loads(body)['type']=='ready'
            def browser_peer():
                try:
                    while True:
                        opcode,body=read_frame(peer)
                        if opcode==8:return
                        if opcode==9:send_frame(peer,body,10);continue
                        assert opcode==1
                        request=json.loads(body);state['bridge'].append(request['action'])
                        action=request['action']
                        if action=='click':peer.close();return
                        if action=='evaluate':assert request['args']['expression']=='location.href';result=site
                        elif action in ('open','navigate'):
                            assert children_started.wait(8),'Both child model requests were not active'
                            state['concurrent_browser_and_children']=True;browser_opened.set();result={'url':site}
                        elif action=='snapshot':result={'url':site,'title':'MOCK_BROWSER_ONLY','text':'SYNTHETIC_DOM_FIXTURE','elements':[]}
                        else:raise AssertionError('Unexpected bridge action '+action)
                        send_frame(peer,{'type':'response','id':request['id'],'ok':True,'result':result})
                except Exception as exc:state['errors'].append(str(exc))
            worker=threading.Thread(target=browser_peer,daemon=True);worker.start()
            cli.send(f'/scope allow {site} read fixture')
            cli.send('Run the explicitly authorized integrated synthetic research fixture.',approvals=5)
            assert state['gets']==1 and state['children']=={'a':3,'b':3},state
            assert state['concurrent_browser_and_children'] is True
            assert state['bridge'].count('open')==1 and state['bridge'].count('snapshot')==1,state
            saved_path=next((app/'sessions').glob('*.json'));saved=json.loads(saved_path.read_text());ident=saved['id']
            values=[json.loads(m['content']) for m in saved['messages'] if m['role']=='tool']
            continuity=next(v for v in reversed(values) if 'delegate_evidence' in v)
            evidence=continuity['delegate_evidence'];assert len(evidence['agents'])==2
            assert all(len(a['evidence'])==1 and a['evidence_revalidation_rejected']==2 for a in evidence['agents']),evidence
            assert continuity['browser_connection']['ready'] is True
            before_bridge=len(state['bridge']);before_gets=state['gets'];cli.send('/scope clear');state.update(phase='revoked',parent=0)
            cli.send('Attempt the scripted negative scope checks; do not expand scope.')
            assert len(state['bridge'])==before_bridge and state['gets']==before_gets,'Revoked scope dispatched I/O'
            saved=json.loads(saved_path.read_text());native={m.get('tool_call_id'):json.loads(m['content']) for m in saved['messages'] if m['role']=='tool'}
            assert native['p-revoked-fetch']['ok'] is False and native['p-revoked-browser']['ok'] is False
            cli.send(f'/scope allow {site} mutate fixture');state.update(phase='disconnect',parent=0)
            cli.send('Execute the one authorized synthetic click; preserve any uncertain outcome.',approvals=1)
            worker.join(timeout=3);assert not worker.is_alive();assert state['bridge'].count('click')==1
            saved=json.loads(saved_path.read_text());native={m.get('tool_call_id'):json.loads(m['content']) for m in saved['messages'] if m['role']=='tool'}
            assert native['p-unknown-click']['execution_status']=='unknown' and native['p-unknown-click']['automatic_retry'] is False
            cli.close();before=(state['packets'],state['gets'],len(state['bridge']),dict(state['children']))
            cli=CLI(base+['--resume',ident,'chat'],env,workspace)
            status=next(v for v in objects(cli.send('/browser status')) if 'reconnect_pending' in v)
            assert status['ready'] is False and status['reconnect_pending'] is True,status
            for command in ['/audit-report','/report']:
                report=next(v for v in objects(cli.send(command)) if 'delegate_evidence' in v)
                assert sum(len(a['evidence']) for a in report['delegate_evidence']['agents'])==2
                assert report['delegate_evidence']['actions_replayed']==0
            child_path=next(p for p in (app/'sessions'/'delegates'/ident).glob('*.json') if p.name!='budget.json')
            original=child_path.read_bytes();child=json.loads(original)
            child['observations'][0]['result_sha256']=None;child_path.write_text(json.dumps(child))
            changed=next(v for v in objects(cli.send('/audit-report')) if 'delegate_evidence' in v)['delegate_evidence']
            assert sum(len(a['evidence']) for a in changed['agents'])==1
            assert changed['source_consistency']['unverified_or_omitted_child_evidence']==5,changed
            child_path.write_bytes(original)
            restored=next(v for v in objects(cli.send('/audit-report')) if 'delegate_evidence' in v)['delegate_evidence']
            assert sum(len(a['evidence']) for a in restored['agents'])==2
            cli.send('/scope clear');cli.close()
            assert before==(state['packets'],state['gets'],len(state['bridge']),dict(state['children'])),'Restart/report caused I/O replay'
            assert not state['errors'],state['errors']
            print(json.dumps({'status':'passed','browser_kind':'authenticated_mock_peer_not_real_browser','independent_children':2,'concurrent_child_requests_during_browser_dispatch':2,'child_browser_and_cross_file_denials':4,'http_gets':state['gets'],'dispatched_clicks':1,'unknown_click_not_replayed':True,'revocation_prevented_io':True,'live_report_checkpoint_tamper_rejected_and_restored':True,'restart_report_provider_requests':0,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}))
    finally:
        if cli and cli.process.poll() is None:cli.process.kill();cli.process.wait(timeout=5)
        if peer:
            try:peer.close()
            except OSError:pass
        server.shutdown();server.server_close()


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);main(p.parse_args().binary.resolve())
