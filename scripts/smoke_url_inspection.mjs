// Synthetic HTTP fixtures + real Chromium. Termux package steps are simulated,
// not a physical-Android test. No model provider or package repository is called.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import {spawn} from 'node:child_process';
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'bb-url-inspection-'));
const binary=path.resolve(process.env.BLUE_DRAGON_TEST_BINARY||'target/release/blue-dragon');
const chrome=process.env.CHROMIUM_BIN||'/usr/bin/chromium';
let modelRequests=[],submissions=0,children=[];
const server=http.createServer(async(req,res)=>{
 if(req.url==='/v1/chat/completions'){
  let body='';for await(const chunk of req)body+=chunk;const request=JSON.parse(body);modelRequests.push(request);
  res.setHeader('Content-Type','application/json');
  res.end(JSON.stringify({choices:[{message:{role:'assistant',content:'Fixture review complete; source/sink indicators require execution verification.'}}]}));
 }else if(req.url==='/app.js'){
  res.setHeader('Content-Type','application/javascript');res.end('console.warn("URL_CONSOLE_FIXTURE"); function example(){const q=location.hash; document.querySelector("#out").innerHTML=q;}');
 }else if(req.url.startsWith('/dom')){res.setHeader('Content-Type','text/html');res.end('<title>DOM fixture</title><form><input name="search"></form><div id="out"></div><script>const value=decodeURIComponent(location.hash.slice(1)) || new URLSearchParams(location.search).get("q") || ""; document.querySelector("#out").'+(req.url.startsWith('/dom-safe')?'textContent':'innerHTML')+'=value;</script>');
 }else if(req.url==='/landing'){res.setHeader('Content-Type','text/html');res.end('<title>Landing fixture</title><a href="https://outside.invalid/register">Register elsewhere</a><a href="/register">Register here</a>');
 }else if(req.url==='/submit'){submissions++;res.end('must not be submitted automatically');}
 else{res.setHeader('Content-Security-Policy',"default-src 'self'; script-src 'self'");res.setHeader('Content-Type','text/html');res.end('<!doctype html><title>URL fixture</title><h1>Real browser page</h1><form method="post" action="/submit"><input type="email" name="email" required><input type="password" name="password"><button>Register</button></form><div id="out"></div><script src="/app.js"></script>');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const site=`http://127.0.0.1:${server.address().port}`;
const quote=s=>"'"+s.replaceAll("'","'\\''")+"'";
const wrapper=path.join(temp,'chrome-fixture');
fs.writeFileSync(wrapper,`#!/bin/sh\nexec ${quote(chrome)} --no-sandbox --disable-dev-shm-usage "$@"\n`,{mode:0o700});
const baseEnv=Object.fromEntries(Object.entries(process.env).filter(([k])=>!/^(BLUE_DRAGON_|OPENAI_|ANTHROPIC_|OPENROUTER_|KILO_|TERMUX_)/.test(k)));
delete baseEnv.DISPLAY;delete baseEnv.WAYLAND_DISPLAY;delete baseEnv.PREFIX;
const env={...baseEnv,BLUE_DRAGON_HOME:path.join(temp,'state'),BLUE_DRAGON_BROWSER_BIN:wrapper};
async function run(args,variables=env,input=''){
 const p=spawn(binary,['--root',temp,'--base-url',site+'/v1','--model','fixture','--no-stream',...args],{env:variables,stdio:['pipe','pipe','pipe']});children.push(p);
 let stdout='',stderr='';p.stdout.on('data',c=>stdout+=c);p.stderr.on('data',c=>stderr+=c);p.stdin.end(input);
 const timer=setTimeout(()=>p.kill('SIGKILL'),60000);const code=await new Promise(r=>p.once('exit',r));clearTimeout(timer);
 return {code,stdout,stderr};
}
try{
 // No browser command, port, token, API key or approval response supplied.
 let result=await run([site]);assert.equal(result.code,0,result.stderr);assert(!result.stdout.includes('APPROVAL REQUIRED'));
 let evidence=JSON.parse(modelRequests.at(-1).messages.at(-1).content.split('Evidence:\n')[1]);
 assert.equal(evidence.page.title,'URL fixture');assert.equal(evidence.page.forms[0].method,'post');
 assert(evidence.page.forms[0].fields.some(f=>f.type==='password'));
 assert(evidence.network.entries.some(e=>e.security_headers?.['content-security-policy']));
 assert(evidence.console.entries.some(e=>e.text.includes('URL_CONSOLE_FIXTURE')));
 assert(evidence.javascript_review.some(e=>e.indicators?.some(i=>i.token==='innerHTML')));
 assert.equal(submissions,0);assert(!JSON.stringify(evidence.page.forms).includes('value'));
 const sessions=fs.readdirSync(path.join(env.BLUE_DRAGON_HOME,'sessions'));
 const report=JSON.parse(fs.readFileSync(path.join(env.BLUE_DRAGON_HOME,'sessions',sessions.at(-1)))).last_report;
 assert(report.actions.some(a=>a.tool==='browser_url_inspection'&&a.ok));
 result=await run(['chat'],env,site+'\n/exit\n');assert.equal(result.code,0,result.stderr);assert(!result.stdout.includes('APPROVAL REQUIRED'));
 result=await run(['inspect',site+'/landing']);assert.equal(result.code,0,result.stderr);
 evidence=JSON.parse(modelRequests.at(-1).messages.at(-1).content.split('Evidence:\n')[1]);
 assert.equal(evidence.page.url,site+'/register');assert.equal(evidence.landing_page.title,'Landing fixture');
 result=await run(['chat'],env,site+'\n/browser do {"action":"snapshot"}\nn\n/exit\n');
 assert.equal(result.code,0,result.stderr);assert(result.stdout.includes('APPROVAL REQUIRED'),'Read grant must end with the URL turn');assert(result.stdout.includes('"denied": true'));
 result=await run(['inspect',site+'/dom?q=hello']);assert.equal(result.code,0,result.stderr);
 evidence=JSON.parse(modelRequests.at(-1).messages.at(-1).content.split('Evidence:\n')[1]);
 assert.equal(evidence.dom_checks.attempts.length,2);
 assert(evidence.dom_checks.attempts.some(a=>a.vector==='fragment'&&a.client_side_execution_observed));
 assert(evidence.dom_checks.attempts.some(a=>a.vector==='query:q'&&a.script_execution_observed&&a.source_classification==='client_or_server_origin_not_determined'));
 result=await run(['inspect',site+'/dom-safe?q=hello']);assert.equal(result.code,0,result.stderr);
 evidence=JSON.parse(modelRequests.at(-1).messages.at(-1).content.split('Evidence:\n')[1]);
 assert.equal(evidence.dom_checks.attempts.length,2);assert(evidence.dom_checks.attempts.every(a=>!a.script_execution_observed));
 const requestsBefore=modelRequests.length;
 result=await run(['--read-only','inspect',site]);assert.notEqual(result.code,0);assert.equal(modelRequests.length,requestsBefore);
 result=await run(['inspect','file:///etc/passwd']);assert.notEqual(result.code,0);assert.equal(modelRequests.length,requestsBefore);
 // First-use orchestration and reuse, using a stub package manager only.
 const fakeBin=path.join(temp,'bin');fs.mkdirSync(fakeBin);
 const browserPath=path.join(fakeBin,'installed-chromium');const calls=path.join(temp,'pkg-calls');
 fs.writeFileSync(path.join(fakeBin,'pkg'),'#!/bin/sh\nprintf "%s\\n" "$*" >> "$TEST_PKG_CALLS"\nif [ "$3" = chromium ]; then cp "$TEST_BROWSER_WRAPPER" "$BLUE_DRAGON_BROWSER_BIN"; chmod 700 "$BLUE_DRAGON_BROWSER_BIN"; fi\n',{mode:0o700});
 const termuxEnv={...env,TERMUX_VERSION:'fixture',PATH:fakeBin+path.delimiter+baseEnv.PATH,BLUE_DRAGON_BROWSER_BIN:browserPath,TEST_BROWSER_WRAPPER:wrapper,TEST_PKG_CALLS:calls};
 result=await run(['inspect',site],termuxEnv);assert.equal(result.code,0,result.stderr);
 assert.deepEqual(fs.readFileSync(calls,'utf8').trim().split('\n'),['install -y x11-repo','install -y chromium']);
 result=await run(['inspect',site],termuxEnv);assert.equal(result.code,0,result.stderr);
 assert.equal(fs.readFileSync(calls,'utf8').trim().split('\n').length,2,'Installed browser must be reused');
 assert.equal(submissions,0);
 console.log('URL-only CLI/chat: automatic real browser, loaded DOM/forms, JS source hints, network/console, no approval prompts, benign DOM canary positive/negative fixtures, no form submission, scoped grant expiry, read-only refusal; simulated Termux package setup/reuse passed');
}finally{children.forEach(p=>p.kill('SIGKILL'));await new Promise(r=>server.close(r));fs.rmSync(temp,{recursive:true,force:true});}
