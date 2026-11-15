// Execute actual extension event collection with mocked browser APIs, no browser/network.
import {readFileSync,writeFileSync} from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const listeners={};
const event=name=>({addListener:(callback,filter,extra)=>{listeners[name]={callback,filter,extra};}});
const api={webRequest:{onBeforeRequest:event('request'),onCompleted:event('response'),onErrorOccurred:event('error')},
 tabs:{onRemoved:event('removed'),get:async()=>({url:'https://example.test/'})},action:{onClicked:event('click')},
 runtime:{openOptionsPage(){}},storage:{local:{get:async()=>({})},onChanged:event('storage')}};
const context=vm.createContext({browser:api,URL,TextEncoder,console,setInterval:()=>{},clearTimeout:()=>{}});
vm.runInContext(readFileSync('browser-extension/shared/background.js','utf8'),context);
vm.runInContext('managed.add(7);currentTab=7;',context);
assert.deepEqual(Array.from(listeners.response.extra),['responseHeaders']);
assert.equal(listeners.request.extra,undefined);
const cases=JSON.parse(readFileSync('scripts/fixtures/policy-header-cases.json','utf8'));
for(const test of cases){
 const result=context.policyIndicators(test.headers);
 assert.equal(result.available,test.available,test.name);
 assert.equal(result.complete,test.complete,test.name);
 assert(!JSON.stringify(result).includes('PRIVATE_'));
}
const normal=cases.find(c=>c.name==='policies').headers;
const base={tabId:7,requestId:'r',method:'GET',statusCode:200,type:'main_frame'};
listeners.response.callback({...base,tabId:999,url:'https://private.invalid/',responseHeaders:normal});
listeners.response.callback({...base,url:'https://example.test/',responseHeaders:normal});
listeners.response.callback({...base,requestId:'duplicate',url:'https://example.test/duplicate',responseHeaders:cases.find(c=>c.name==='duplicate').headers});
listeners.response.callback({...base,requestId:'missing',url:'https://example.test/missing'});
const result=JSON.parse(JSON.stringify(await context.execute('network',{})));
assert.equal(result.entries.length,3); // Unmanaged tabs are ignored.
assert(!JSON.stringify(result).includes('PRIVATE_'));
assert(!JSON.stringify(result).includes('security_headers"'));
assert.equal(result.entries[0].security_header_indicators.csp_header_frame_directive_seen,true);
assert.equal(result.entries[0].security_header_indicators.report_only_frame_directive_seen,true);
const path='scripts/fixtures/browser-policy-network.json';
if(process.argv.includes('--write-fixture'))writeFileSync(path,JSON.stringify(result)+'\n');
else assert.deepEqual(result,JSON.parse(readFileSync(path,'utf8')));
listeners.response.callback({...base,url:'https://example.test/'+ 'x'.repeat(2100),responseHeaders:normal});
assert.equal((await context.execute('network',{})).entries.at(-1).url_truncated,true);
const truncatedURL=(await context.execute('network',{})).entries.at(-1).url;
await assert.rejects(context.execute('source',{url:truncatedURL}),/must have been observed/);
console.log('Extension policy collector, redaction, bounds and producer fixture passed');

for(let i=0;i<350;i++)listeners.response.callback({...base,requestId:String(i),url:'https://example.test/'+ 'x'.repeat(1900),responseHeaders:normal});
const bounded=await context.execute('network',{});
assert(bounded.entries_omitted>0);
assert(bounded.ring_entries_omitted>0);
assert(JSON.stringify(bounded).length<181000);
console.log('Network export byte budget and ring omissions passed');
