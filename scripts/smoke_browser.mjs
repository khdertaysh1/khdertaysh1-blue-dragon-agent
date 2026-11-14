// Real-browser integration; only synthetic localhost pages, never live targets.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import {spawn} from 'node:child_process';
const firefoxMode=process.argv.includes('--firefox');
const extensionMode=process.argv.includes('--extension')||firefoxMode;
const root=process.cwd(),temp=fs.mkdtempSync(path.join(os.tmpdir(),'bb-browser-proof-'));
const binary=path.resolve(process.env.BLUE_DRAGON_TEST_BINARY||'target/release/blue-dragon');
const chrome=process.env.CHROMIUM_BIN||'/usr/bin/chromium';
const extension=path.join(root,'target/browser-extensions/chromium');
let browser,agent,inspector,webdriverSession,driverURL;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,timeout=20000){const end=Date.now()+timeout;while(Date.now()<end){const v=await fn();if(v)return v;await sleep(50);}throw Error('Test deadline exceeded');}
let apiHits=0;
const server=http.createServer((req,res)=>{
 if(req.url==='/api'){apiHits++;res.setHeader('Content-Type','application/json');res.end(JSON.stringify({message:'Hello Browser42'}));}
 else if(req.url==='/app.js'){res.setHeader('Content-Type','application/javascript');res.end(`console.log('BB_BROWSER_TEST');window.__freshLoad=location.hash;document.querySelector('#send').onclick=async()=>{const r=await fetch('/api');document.querySelector('#out').textContent=(await r.json()).message;};`);}
 else if(req.url==='/unmanaged'){res.end('<h1>UNMANAGED_SECRET</h1>');}
 else{res.setHeader('Content-Type','text/html');res.end('<!doctype html><title>Blue Dragon fixture</title><h1>BB fixture ready</h1><input id="name"><button id="send">Send</button><div id="out"></div><script src="/app.js"></script>');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const site=`http://127.0.0.1:${server.address().port}`;
try{
 let port;
 if(firefoxMode){
  const allocator=http.createServer();await new Promise(r=>allocator.listen(0,'127.0.0.1',r));const driverPort=allocator.address().port;await new Promise(r=>allocator.close(r));
  driverURL=`http://127.0.0.1:${driverPort}`;
  browser=spawn(process.env.GECKODRIVER_BIN||'/tmp/blue-dragon-firefox-test/geckodriver',['--host','127.0.0.1','--port',String(driverPort),'--allow-system-access'],{stdio:['ignore','ignore','pipe']});
  await until(async()=>{try{return(await fetch(driverURL+'/status')).ok;}catch{return false;}});
  const response=await fetch(driverURL+'/session',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({capabilities:{alwaysMatch:{browserName:'firefox','moz:firefoxOptions':{binary:process.env.FIREFOX_BIN||'/tmp/blue-dragon-firefox-test/firefox/firefox',args:['-headless','-no-remote'],prefs:{'extensions.webextensions.uuids':JSON.stringify({'blue-dragon-bridge@khdertaysh1.github.io':'10000000-0000-4000-8000-000000000001'})}}}}}),signal:AbortSignal.timeout(60000)});
  const data=await response.json();assert(!data.value.error,JSON.stringify(data));webdriverSession=data.value.sessionId;
  await wd('/moz/addon/install',{path:path.join(root,'target/browser-extensions/blue-dragon-browser-firefox.xpi'),temporary:true});
 }else{
 const profile=path.join(temp,'chrome');fs.mkdirSync(profile);
 const flags=['--headless=new','--no-sandbox','--disable-dev-shm-usage','--remote-debugging-address=127.0.0.1','--remote-debugging-port=0',`--user-data-dir=${profile}`];
 if(extensionMode)flags.push(`--disable-extensions-except=${extension}`,`--load-extension=${extension}`);
 browser=spawn(chrome,[...flags,`${site}/unmanaged`],{stdio:['ignore','ignore','pipe']});
 let browserErrors='';browser.stderr.on('data',b=>browserErrors+=b);
 port=await until(()=>{try{return Number(fs.readFileSync(path.join(profile,'DevToolsActivePort'),'utf8').split('\n')[0]);}catch{return false;}}).catch(e=>{throw Error(e.message+browserErrors.slice(-1000));});
 }
 const env=Object.fromEntries(Object.entries(process.env).filter(([k])=>! /^(BLUE_DRAGON_|OPENAI_|ANTHROPIC_|OPENROUTER_|KILO_)/.test(k)));
 env.BLUE_DRAGON_HOME=path.join(temp,'state');
 agent=spawn(binary,['--root',temp,'--provider','free','chat'],{env,stdio:['pipe','pipe','pipe']});
 let output='',errors='';agent.stdout.on('data',b=>output+=b.toString());agent.stderr.on('data',b=>errors+=b.toString());
 await until(()=>output.includes('blue-dragon> '));
 async function send(command,approval=false){
  if(process.env.BB_BROWSER_VERBOSE)console.log("Browser test:",command.slice(0,150));
  const offset=output.length,errorOffset=errors.length;
  agent.stdin.write(command+'\n'+(approval?'y\n':''));
  await until(()=>output.slice(offset).includes('blue-dragon> '),40000);
  const text=output.slice(offset);
  const err=errors.slice(errorOffset);assert.equal(err,'',err);
  const start=text.indexOf('{\n'),end=text.lastIndexOf('\n}');
  return{data:start>=0&&end>=start?JSON.parse(text.slice(start,end+2)):null,text};
 }
 if(extensionMode){
  const paired=(await send('/browser start 0')).data;
  if(firefoxMode){
   await wd('/moz/context',{context:'chrome'});
   await wd('/execute/sync',{script:`const w=Services.wm.getMostRecentWindow('navigator:browser');const p=WebExtensionPolicy.getByID('blue-dragon-bridge@khdertaysh1.github.io');w.gBrowser.selectedTab=w.gBrowser.addTab(p.getURL('options.html'),{triggeringPrincipal:Services.scriptSecurityManager.getSystemPrincipal()});return true;`,args:[]});
   await wd('/moz/context',{context:'content'});
   await sleep(300);
   const handles=(await(await fetch(driverURL+'/session/'+webdriverSession+'/window/handles')).json()).value;
   await wd('/window',{handle:handles.at(-1)});
   const config={token:paired.pairing_token,port:Number(new URL(paired.url).port)};
   await wd('/execute/sync',{script:`document.querySelector('#token').value=arguments[0].token;document.querySelector('#port').value=arguments[0].port;document.querySelector('#pair').requestSubmit();return true;`,args:[config]});
  }else{
  const worker=await until(async()=>{const list=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();return list.find(t=>t.type==='service_worker'&&t.url.endsWith('/worker.js'));});
  const optionsURL=new URL('options.html',worker.url).href;
  const optionsTab=await(await fetch(`http://127.0.0.1:${port}/json/new?${optionsURL}`,{method:'PUT'})).json();
  inspector=new WebSocket(optionsTab.webSocketDebuggerUrl);
  await new Promise((resolve,reject)=>{inspector.addEventListener('open',resolve,{once:true});inspector.addEventListener('error',reject,{once:true});});
  let sequence=0;
  async function evaluateOptions(expression){
   const id=++sequence;
   const reply=new Promise((resolve,reject)=>{
    const listener=event=>{const data=JSON.parse(event.data);if(data.id!==id)return;inspector.removeEventListener('message',listener);if(data.error||data.result?.exceptionDetails)reject(Error(JSON.stringify(data)));else resolve(data.result.result.value);};
    inspector.addEventListener('message',listener);
   });
   inspector.send(JSON.stringify({id,method:'Runtime.evaluate',params:{expression,awaitPromise:true,returnByValue:true}}));
   return await reply;
  }
  await until(()=>evaluateOptions("document.readyState==='complete' && !!document.querySelector('#pair')"));
  const config={token:paired.pairing_token,port:Number(new URL(paired.url).port)};
  await evaluateOptions(`(()=>{const c=${JSON.stringify(config)};document.querySelector('#token').value=c.token;document.querySelector('#port').value=c.port;document.querySelector('#pair').requestSubmit();return true;})()`);

  }
  await until(async()=>{await sleep(100);return(await send('/browser status')).data?.ready;});
 }else{assert.equal((await send(`/browser attach ${port}`)).data.ready,true);}
 async function action(args){const result=(await send('/browser do '+JSON.stringify(args),true)).data;assert.equal(result?.ok,true,JSON.stringify(result));return result.result;}
 await action({action:'open',url:site});
 const snapshot=await action({action:'snapshot'});assert.equal(snapshot.title,'Blue Dragon fixture');assert(snapshot.elements.some(e=>e.selector==='#send'));assert(!snapshot.text.includes('UNMANAGED_SECRET'));
 await action({action:'fill',selector:'#name',value:'Browser42'});
 assert.equal(await action({action:'evaluate',expression:'document.querySelector("#name").value'}),'Browser42');
 if(extensionMode)assert.equal(await action({action:'evaluate',expression:'typeof chrome === "undefined" || typeof chrome.storage === "undefined"'}),true,'MAIN world must not expose extension storage');
 await action({action:'click',selector:'#send'});await until(()=>apiHits>0);await sleep(150);
 assert((await action({action:'snapshot'})).text.includes('Hello Browser42'));
 const network=await action({action:'network'});assert(network.entries.some(e=>e.url?.includes('/api')));assert(!network.entries.some(e=>e.url?.includes('/unmanaged')));
 const script=network.entries.find(e=>e.event==='response'&&e.url.endsWith('/app.js'));assert(script,'app.js must be observed');
 const source=await action(extensionMode?{action:'source',url:script.url}:{action:'source',request_id:script.request_id});assert(source.body.includes('BB_BROWSER_TEST'));
 await action({action:'evaluate',expression:'console.warn("BRIDGE_LATE"); 42'});
 assert((await action({action:'console'})).entries.some(e=>(e.text||'').includes('BRIDGE_LATE')));
 const shot=await action({action:'screenshot'});assert(fs.statSync(shot.path).size>100);assert.equal(fs.readFileSync(shot.path)[0],0xff);
 await action({action:'navigate',url:site+'#fresh-document-test',fresh_document:true});
 assert.equal(await action({action:'evaluate',expression:'window.__freshLoad'}),'#fresh-document-test');
 const rotation=(await send('/rotation')).data;assert.equal(rotation.mode,'Blue Free automatic');
 agent.stdin.write('/exit\n');await new Promise(resolve=>agent.once('exit',resolve));
 console.log(`${firefoxMode?'FIREFOX EXTENSION':extensionMode?'CHROMIUM EXTENSION':'CDP'} real browser: open/DOM/fill/click/JS/network/source/console/screenshot, managed-tab isolation and approved CLI roundtrips passed`);
}catch(error){console.error(error);process.exitCode=1;}
finally{if(webdriverSession){await fetch(driverURL+'/session/'+webdriverSession,{method:'DELETE'}).catch(()=>{});}inspector?.close();agent?.kill('SIGKILL');browser?.kill('SIGKILL');await new Promise(r=>server.close(r));fs.rmSync(temp,{recursive:true,force:true});}

async function wd(endpoint,body){
 const response=await fetch(driverURL+'/session/'+webdriverSession+endpoint,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(30000)});
 const data=await response.json();assert(!data.value?.error,JSON.stringify(data));return data.value;
}
