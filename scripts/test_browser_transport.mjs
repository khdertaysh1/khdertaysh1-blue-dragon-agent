// Actual injected function + extension transport under browser API fixtures.
// This exercises transport semantics, not Chromium/Firefox or a DOM engine.
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
let injected, substitute, dispatches=0;
const event=()=>({addListener(){}});
const api={webRequest:{onBeforeRequest:event(),onCompleted:event(),onErrorOccurred:event()},
 tabs:{onRemoved:event(),get:async()=>({url:'https://fixture.test/',status:'complete'})},
 action:{onClicked:event()},runtime:{openOptionsPage(){}},
 storage:{local:{get:async()=>({})},onChanged:event()},
 scripting:{executeScript:async request=>{
   dispatches++;
   assert.equal(request.target.tabId,7);assert.equal(request.world,'MAIN');
   assert.equal(request.args[2],true);
   if(substitute)return substitute();
   injected=request.func.toString();
   page.args=request.args;
   const value=await vm.runInContext(`(${injected})(...args)`,page);
   // Model JSON-safe transport and verify no outer closure was required.
   return [{frameId:0,result:JSON.parse(JSON.stringify(value))}];
 }}};
const page=vm.createContext({document:{querySelectorAll:()=>[]},window:{__bbStudioConsole:[]},console});
page.window.eval=expression=>vm.runInContext(expression,page);
const context=vm.createContext({chrome:api,URL,TextEncoder,console,setInterval:()=>{},clearTimeout:()=>{}});
vm.runInContext(readFileSync('browser-extension/shared/page.js','utf8'),context);
vm.runInContext(readFileSync('browser-extension/shared/background.js','utf8'),context);
vm.runInContext('managed.add(7);currentTab=7;',context);
const evaluate=expression=>context.execute('evaluate',{expression});
for(const [expression,expected] of [['null',null],['undefined',null],['42',42],['false',false],['"🐉"','🐉'],['Promise.resolve(7)',7]]) {
 assert.equal(await evaluate(expression),expected);
}
assert.deepEqual(JSON.parse(JSON.stringify(await evaluate('({ok:false,value:9})'))),{ok:false,value:9});
for(const expression of ['(()=>{throw Error("sync failure")})()','Promise.reject(Error("async failure"))']) {
 const before=dispatches;
 await assert.rejects(()=>evaluate(expression),/failure/);assert.equal(dispatches,before+1);
}
await assert.rejects(()=>evaluate('x'.repeat(16001)),/Invalid expression/);
try { await evaluate('Promise.reject(Error("x".repeat(400)))'); assert.fail('Rejected promise accepted'); }
catch(error) { assert.equal(error.message.length,300); }
await assert.rejects(()=>context.execute('scroll',{delta_y:2001}),/scroll/i);
await assert.rejects(()=>context.execute('scroll',{selector:'#hidden',delta_y:1}),/scroll/i);
for(const response of [undefined,[],[{frameId:0}],[{frameId:0,result:null}],
 [{frameId:0,result:{blueDragonPageEnvelope:1,ok:true}}],
 [{frameId:0,result:{blueDragonPageEnvelope:1,ok:true,value:undefined}}],
 [{frameId:0,result:{blueDragonPageEnvelope:1,ok:'true',value:1}}],
 [{frameId:0,result:{blueDragonPageEnvelope:1,ok:false}}],
 [{frameId:0,result:{blueDragonPageEnvelope:1,ok:true,value:1,error:'ambiguous'}}],
 [{frameId:1,result:{blueDragonPageEnvelope:1,ok:true,value:1}}],
 [{frameId:0,error:'transport failure',result:{blueDragonPageEnvelope:1,ok:true,value:1}}],
 [{frameId:0,result:{blueDragonPageEnvelope:1,ok:true,value:1}},{frameId:2,result:null}]]) {
 substitute=()=>response;
 await assert.rejects(()=>evaluate('1'),/execution|envelope/i);
}
substitute=()=>{throw Error('executeScript rejected');};
await assert.rejects(()=>evaluate('1'),/executeScript rejected/);
console.log('Extension transport: explicit envelopes, null/undefined, Promise failures, malformed/missing results and one-dispatch failure checks passed (mock APIs)');
