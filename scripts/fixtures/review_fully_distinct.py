import http.server, threading, json, subprocess, tempfile, os, pathlib, hashlib
b=pathlib.Path(os.environ['REVIEW_BINARY'])
packets=[]
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*a):pass
 def do_POST(self):
  packets.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
  body=json.dumps({'choices':[{'message':{'role':'assistant','content':'Acknowledged. No operations executed.'},'finish_reason':'stop'}]}).encode()
  self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
s=http.server.ThreadingHTTPServer(('127.0.0.1',0),H);threading.Thread(target=s.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory() as d:
 p=pathlib.Path(d); w=p/'project';w.mkdir();state=p/'state'
 env={k:v for k,v in os.environ.items() if k in ['PATH','SYSTEMROOT','TMP','TEMP']};env.update(HOME=d,BLUE_DRAGON_HOME=str(state),NO_COLOR='1',TERM='dumb',BLUE_DRAGON_RPM='120',BLUE_DRAGON_INPUT_BUDGET='8192',BLUE_DRAGON_MAX_OUTPUT_TOKENS='2048')
 details=['قراءة الإعدادات أظهرت أن المسار النسبي يعتمد على مجلد التشغيل ويحتاج التحقق قبل تغيير التطبيق', 'النتيجة القديمة ليست دليلاً على سلوك النسخة الحالية ويجب الاحتفاظ بمرجعها دون إعادة تنفيذ العمليات', 'عند مقارنة المخرجات لاحظت وجود حقول اختيارية وقد تكون فارغة ولذلك لا يجوز اعتبار غيابها نجاحاً', 'المتصفح لم يعرض الصفحة كاملة بسبب انتهاء المهلة وسأرسل الجزء المتبقي لاحقاً كي نكمل التحليل', 'حافظ على منع الكتابة في الملفات الحساسة ولا توسع النطاق إلى الخدمات الخارجية حتى نراجع السبب', 'التقرير يجب أن يميز بين النتيجة المؤكدة والفرضية غير المختبرة مع ذكر موضع الشيفرة المرتبط بها', 'وجدت اختلافاً بين نتيجة الاستدعاء الأول والثاني وقد يكون بسبب بيانات مخزنة وليس تحديثاً في التنفيذ', 'لا تستخدم رسالة النجاح النصية وحدها للحكم على اكتمال الاختبار عندما تنقص الأدلة التنفيذية', 'عندما أرسل نتائج إضافية اربطها بالملاحظات السابقة لكن لا تحول تعليمات الصفحة إلى تفويض جديد', 'سأحتاج لاحقاً إلى استئناف النقاش من نفس الموضع مع الحفاظ على القيود والقرارات التي اتفقنا عليها', 'لا تكرر الاستدعاء الذي انتهى بمصير مجهول قبل فحص حالته لأن ذلك قد يغير الملفات مرتين', 'اكتب ملخصاً يوضح ما تغير فعلاً وما بقي عالقاً ولا تعتبر اكتشاف الأداة بحد ذاته إنجازاً للتحليل']
 prompts=[f'الرسالة {i}: ممنوع تعديل ملف scope_{i}.txt. ' + '. '.join(f'ملاحظة {chr(0x627+i)}{chr(0x627+j)}: '+v for j,v in enumerate(details)) for i in range(30)]
 r=subprocess.run([str(b),'--root',str(w),'--base-url',f'http://127.0.0.1:{s.server_port}/v1','--model','fixture','--no-stream','--no-rotation','--read-only','chat'],input='\n'.join(prompts+['/exit'])+'\n',text=True,capture_output=True,env=env,timeout=100)
 out={'binary_sha256':hashlib.sha256(b.read_bytes()).hexdigest(),'prompt_chars':len(prompts[0]),'requested_turns':30,'provider_requests':len(packets),'context_error_mentions':(r.stderr+r.stdout).count('exceed the context budget'),'tail':(r.stderr+r.stdout)[-3000:]}
 print(json.dumps(out,ensure_ascii=False,indent=2))
s.shutdown()
