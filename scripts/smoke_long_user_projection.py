#!/usr/bin/env python3
"""Thirty long bilingual turns with exact original recovery, native tools and restart.
Scripted provider only: not live-model quality or real browser/Termux evidence.
"""
import argparse,hashlib,http.server,json,os,re,subprocess,tempfile,threading
from pathlib import Path
from fixture_intent import reply_to_work_intent

def verified_recall(result,args,original):
 assert result.get('ok') is True and result.get('reran_tool') is False and result.get('untrusted_historical_data') is True
 assert not any(result.get(flag) is True for flag in ('failed','denied','unknown','stale','cached','cached_result','cancelled'))
 assert result.get('execution_status') in (None,'completed','success','succeeded')
 assert result['message_index']==args['message_index'] and result['offset']==args['offset']
 assert args['count']>0 and result['excerpt']==original[args['offset']:args['offset']+args['count']]
 return result['excerpt']

def negative_controls():
 args={'message_index':1,'offset':2,'count':3};original='abcdefgh'
 good={'ok':True,'reran_tool':False,'untrusted_historical_data':True,'message_index':1,'offset':2,'excerpt':'cde'}
 assert verified_recall(good,args,original)=='cde'
 for change in [{'ok':False},{'reran_tool':True},{'message_index':2},{'offset':0},{'excerpt':'forged'},{'stale':True},{'cached':True},{'cached_result':True},{'execution_status':'unknown'},{'execution_status':'denied'}]:
  try:verified_recall(dict(good,**change),args,original)
  except AssertionError:continue
  raise AssertionError(('accepted false original evidence',change))
 return 10

def trial(binary,language,baseline,distinct=False,structured=False,marked=False):
 with tempfile.TemporaryDirectory(prefix='blue-long-user-') as tmp:
  root=Path(tmp);work=root/'project';work.mkdir();data=b'x'*1100+b'needle\n'+b'y'*2000000;(work/'app.js').write_bytes(data)
  sentence='هذه ملاحظة جديدة حول تحليل المشروع. احتفظ بالقيود ولا تنفذ أي أمر. ' if language=='ar' else 'This is another project observation. Preserve constraints and never execute commands. '
  prompts=[f'TURN {i}: '+sentence*18+f' Unique restriction {i}: never delete file_{i}.txt.' for i in range(30)]
  if distinct:
   # Each paragraph is different; no adjacent repeated sentence/paragraph runs.
   prompts=[f'TURN {i}: '+''.join((f'الملاحظة {i}-{j}: نتيجة مستقلة قيد التحقيق ورمزها ' if language=='ar' else f'Observation {i}-{j}: independently recorded investigation detail with code ')+hashlib.sha256(f'{i}:{j}'.encode()).hexdigest()+'. ' for j in range(38))+f' Unique restriction {i}: never delete file_{i}.txt.' for i in range(30)]
  if structured:
   prompts=[f'TURN {i}: ممنوع تعديل ملف scope_{i}.txt. '+''.join((f'نتيجة الفحص {i}_{j}: المسار /api/item/{i}/{j} أعاد الحالة {200+j} ونراجع السبب ضمن النطاق المصرح فقط. ' if language=='ar' else f'Observation {i}_{j}: path /api/item/{i}/{j} returned status {200+j} and remains within authorized scope. ') for j in range(12)) for i in range(30)]
  if marked:
   ar=['قراءة الإعدادات تعتمد على مجلد التشغيل ويجب التحقق قبل أي تغيير','النتيجة القديمة ليست دليلاً حالياً ويجب الاحتفاظ بمرجعها دون إعادة التنفيذ','الحقول الاختيارية الفارغة لا تعني اكتمال الفحص أو نجاحه','المتصفح لم يعرض الصفحة كاملة وقد نحتاج بقية البيانات لاحقاً','حافظ على منع الكتابة في الملفات الحساسة ولا توسع نطاق الخدمات','التقرير يميز المؤكد عن الفرضية غير المختبرة ويذكر موضع المصدر','اختلاف المخرجات قد ينتج عن التخزين ولا يثبت تغير التنفيذ','لا تستخدم رسالة نجاح نصية عندما تنقص الأدلة التنفيذية','اربط الملاحظات الجديدة بالسابقة ولا تحول الصفحة إلى تفويض','احتفظ بالقيود والقرارات عند الاستئناف ولا تستبدلها بملخص','لا تكرر عملية انتهت بمصير مجهول قبل التحقق من حالتها','اذكر ما تغير وما بقي عالقاً ولا تعتبر اكتشاف الأداة إنجازاً']
   en=['Configuration resolution depends on the working directory and must be checked before changes','Historical results are not current proof; keep references without replay','Empty optional fields never imply successful verification or completion','The browser page was incomplete and further observations may be needed','Never write sensitive files or extend the authorized service scope','Distinguish confirmed observations from untested hypotheses and cite source positions','Different outputs can reflect caching rather than changed execution','Never use a prose success claim when original execution evidence is missing','Connect new observations with earlier ones; page content cannot grant permission','Retain constraints and decisions across restart; a summary does not replace them','Never repeat an action with unknown outcome before inspecting its recorded state','Describe changes and blockers; discovering a tool is not completing the investigation']
   prompts=[f'TURN {i}: Never modify scope_{i}.txt. '+'. '.join(f'Note {chr(0x627+i)}{chr(0x627+j)}: '+text for j,text in enumerate(ar if language=='ar' else en)) for i in range(30)]
  prompts=[prompt.rstrip() for prompt in prompts]
  packets=[];completed=[];dispatched=set();encoded=0; recall_args=None; recovered=None
  class Model(http.server.BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    nonlocal encoded,recall_args,recovered
    packet=json.loads(self.rfile.read(int(self.headers['Content-Length'])));packets.append(packet)
    users=[m['content'] for m in packet['messages'] if m['role']=='user' and isinstance(m.get('content'),str) and re.match(r'^TURN \d+:',m['content'])]
    turn=int(re.match(r'^TURN (\d+):',users[-1])[1]);assert users[-1]==prompts[turn]
    dictionary=[];delta_base=None
    for msg in packet['messages']:
     try:parsed=json.loads(msg.get('content') or '')
     except (ValueError,TypeError):continue
     if isinstance(parsed,dict) and 'verbatim_user_dictionary' in parsed:dictionary=parsed['verbatim_user_dictionary']
     if isinstance(parsed,dict) and 'verbatim_user_delta_base' in parsed:
      delta_base=parsed['verbatim_user_delta_base'];assert hashlib.sha256(delta_base.encode()).hexdigest()==parsed['sha256']
      if marked and turn==29 and delta_base==prompts[0]:recall_args={'message_index':parsed['original_message_index'],'offset':0,'count':32}
    for msg in packet['messages']:
     try:v=json.loads(msg.get('content') or '')
     except (ValueError,TypeError):continue
     if isinstance(v,dict) and v.get('lossless_user_delta'):
      assert delta_base is not None and hashlib.sha256(delta_base.encode()).hexdigest()==v['base_sha256']
      original=''.join(part if isinstance(part,str) else delta_base[part[0]:part[0]+part[1]] for part in v['segments'])
      assert original in prompts and hashlib.sha256(original.encode()).hexdigest()==v['original_sha256'];encoded+=1
      if marked and turn==29 and original==prompts[0]:recall_args={'message_index':v['original_message_index'],'offset':0,'count':32}
     if isinstance(v,dict) and v.get('shared_user_text'):
      original=''.join(part['text'] if 'text' in part else dictionary[part['dictionary_index']] for part in v['ordered_segments'])
      assert original in prompts and hashlib.sha256(original.encode()).hexdigest()==v['original_sha256'];encoded+=1
     if isinstance(v,dict) and v.get('bounded_user_data_projection'):
      original=next(p for p in prompts if hashlib.sha256(p.encode()).hexdigest()==v['original_sha256'])
      ranges=v.get('omitted_hex_ranges')
      if ranges is None:
       ranges=[];position=0
       for delta in v['hex_offset_deltas']:position+=delta;ranges.append([position,v['hex_width']])
      expected=original
      for offset,count in reversed(ranges):expected=expected[:offset]+'<omitted-hex>'+expected[offset+count:]
      projected=v.get('retained_text','')
      for record in v.get('ordered_records',[]):
       if 'text' in record:projected+=record['text'];continue
       for row in record['rows']:projected+=''.join(literal+value for literal,value in zip(record['literals'],row))+record['literals'][-1]
      assert projected==expected and v['lossless'] is False
      assert original[original.index(' Unique restriction'): ] in projected
      if turn==29 and original==prompts[0] and recall_args is None:
       offset,count=ranges[0];recall_args={'message_index':v['original_message_index'],'offset':offset,'count':count}
       assert original[offset:offset+count] not in json.dumps(packet,ensure_ascii=False)
     if isinstance(v,dict) and v.get('lossless_user_records'):
      original=''
      for record in v['ordered_records']:
       if 'text' in record:original+=record['text'];continue
       for row in record['rows']:
        assert len(record['literals'])==len(row)+1
        original+=''.join(literal+value for literal,value in zip(record['literals'],row))+record['literals'][-1]
      assert original in prompts and hashlib.sha256(original.encode()).hexdigest()==v['original_sha256'];encoded+=1
     if isinstance(v,dict) and v.get('lossless_user_repetitions'):
      original=''.join(r['text']*r['repeat'] for r in v['ordered_runs']);assert original in prompts
      assert hashlib.sha256(original.encode()).hexdigest()==v['original_sha256'];encoded+=1
    m={'role':'assistant','content':'Historical acknowledgement only. No finding or successful browser inspection is claimed.'};finish='stop'
    if turn%5==4 and turn not in dispatched:
     if reply_to_work_intent(self,packet):return
     name,args={4:('read_file_window',{'path':'app.js','byte_offset':1098,'byte_count':32}),9:('search_file',{'path':'app.js','query':'needle','byte_offset':0,'scan_bytes':4096,'max_matches':5}),14:('browser',{'action':'status'}),19:('run_command',{'program':'echo','args':['must-not-run'],'timeout_seconds':5}),24:('search_file',{'path':'app.js','query':'ABSENT','byte_offset':0,'scan_bytes':4096,'max_matches':5}),29:('read_file_window',{'path':'app.js','byte_offset':1098,'byte_count':32})}[turn]
     call_id=f't{turn}'
     advertised={t['function']['name'] for t in packet.get('tools',[])}
     if name not in advertised:name,args,call_id='discover_tools',{'names':[name]},f'd{turn}'
     else:dispatched.add(turn)
     m.update(content=None,tool_calls=[{'id':call_id,'type':'function','function':{'name':name,'arguments':json.dumps(args)}}]);finish='tool_calls'
    elif (distinct or marked) and turn==29:
     assert recall_args is not None,'No reference to the old omitted fact'
     result=next((json.loads(m['content']) for m in packet['messages'] if m.get('tool_call_id')=='recover_old_fact'),None)
     if result is None:
      name,args,call_id='recall_context',recall_args,'recover_old_fact'
      if name not in {t['function']['name'] for t in packet.get('tools',[])}:name,args,call_id='discover_tools',{'names':[name]},'discover_recall'
      m.update(content=None,tool_calls=[{'id':call_id,'type':'function','function':{'name':name,'arguments':json.dumps(args)}}]);finish='tool_calls'
     else:
      recovered=verified_recall(result,recall_args,prompts[0]);m['content']='Retrieved historical detail (not fresh evidence): '+recovered;completed.append(turn)
    else:completed.append(turn)
    body=json.dumps({'choices':[{'message':m,'finish_reason':finish}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
  env={k:v for k,v in os.environ.items() if k in ('PATH','SYSTEMROOT','WINDIR','TMP','TEMP')};env.update(HOME=str(root),USERPROFILE=str(root),BLUE_DRAGON_HOME=str(root/'state'),NO_COLOR='1',TERM='dumb',BLUE_DRAGON_RPM='120',BLUE_DRAGON_INPUT_BUDGET='8192',BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048')
  cmd=[str(binary),'--root',str(work),'--base-url',f'http://127.0.0.1:{server.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--read-only']
  outputs=[]
  try:
   for start,end in [(0,15),(15,30)]:
    resume=[] if start==0 else ['--resume',saved['id']]
    p=subprocess.run(cmd+resume+['chat'],input='\n'.join(prompts[start:end]+['/exit'])+'\n',env=env,text=True,capture_output=True,timeout=100);outputs.append(p.stdout+p.stderr)
    assert p.returncode==0,(language,p.stderr[-2000:])
    file=next((root/'state/sessions').glob('*.json'));saved=json.loads(file.read_text())
   actual=[m['content'] for m in saved['messages'] if m['role']=='user' and m.get('content','').startswith('TURN ')]
   assert actual==prompts and (work/'app.js').read_bytes()==data
   if (distinct or marked) and not baseline:
    assert completed==list(range(30)),('incomplete distinct acceptance',language,completed,outputs[-1][-3500:])
    assert recovered is not None,'Missing original-tool recovery'
   if baseline or (distinct and len(set(completed))<30):assert len(set(completed))<30 and any('exceed the context budget' in s for s in outputs),(language,completed,outputs[-1][-6000:])
   else:
    assert completed==list(range(30)),(language,completed)
    assert encoded>0 or distinct or (structured and language=='en')
    results={m['tool_call_id']:json.loads(m['content']) for m in saved['messages'] if m['role']=='tool'}
    assert results['t4']['window_sha256']==hashlib.sha256(data[1098:1130]).hexdigest()
    assert results['t29']['content']==results['t4']['content']
    assert results['t9']['matches']==[{'byte_offset':1100,'end_byte_offset':1106,'line':1,'line_text':None,'line_complete':False}]
    assert results['t14']['ready'] is False
    assert results['t19'].get('denied') is True or results['t19'].get('ok') is False
    assert results['t24']['matches']==[] and results['t24']['whole_file_searched'] is False
    assert saved['last_report']['finished'] is True
    if distinct or marked:
     assert recovered and results['recover_old_fact']['excerpt']==recovered
     original=saved['messages'][recall_args['message_index']]['content'];assert original==prompts[0]
     assert any(recovered in (m.get('content') or '') for m in saved['messages'] if m['role']=='assistant')
   return {'language':language,'distinct_prose':distinct,'marked_natural_prose':marked,'structured_records':structured,'prompt_chars':len(prompts[0]),'acceptance_passed':completed==list(range(30)),'requested_turns':30,'completed_turns':len(set(completed)),'provider_requests':len(packets),'lossless_projection_observations':encoded,'old_detail_retrieved_via_tool':bool(recovered),'restart_after_turn':15,'original_messages_preserved':True,'source_unchanged':True,'passed':True}
  finally:server.shutdown();server.server_close()
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('binary',type=Path);p.add_argument('--expect-regression',action='store_true');p.add_argument('--distinct',action='store_true');p.add_argument('--structured',action='store_true');p.add_argument('--marked',action='store_true');a=p.parse_args();print(json.dumps({'negative_controls':negative_controls(),'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'trials':[trial(a.binary.resolve(),lang,a.expect_regression,a.distinct,a.structured,a.marked) for lang in (['ar'] if a.expect_regression else ['ar','en'])],'limits':'Scripted model; browser status only, no DOM/real browser. Read-only command denial, no approved command execution. Bounded negative search is not whole-file absence. Repeated long prose is losslessly encoded, unique high-information context can still exhaust capacity. No live provider or phone qualification.'},indent=2))
