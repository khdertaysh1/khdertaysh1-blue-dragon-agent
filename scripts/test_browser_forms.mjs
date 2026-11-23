// Dependency-free contract tests for the shared CDP/extension page implementation.
import {readFileSync, writeFileSync} from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
let element, clicks = 0, events = [];
class Select {get value(){return this.current;} set value(v){this.current=v;}}
const context = vm.createContext({window:{__bbStudioConsole:[]}, document:{querySelectorAll:()=>element ? [element] : []}, HTMLSelectElement:Select, Event:class {constructor(type){this.type=type;}}, console});
vm.runInContext(readFileSync('browser-extension/shared/page.js','utf8'), context);
const run = (action,args)=>context.blueDragonPage(action,{selector:'#control',...args});
const common = {getClientRects:()=>[{}], matches:()=>false, scrollIntoView(){}, dispatchEvent(e){events.push(e.type);}};
element = {...common,tagName:'INPUT',type:'checkbox',checked:false,click(){clicks++;this.checked=!this.checked;}};
assert.equal(run('set_checked',{checked:true}).matched,true);
assert.equal(clicks,1);
assert.equal(run('set_checked',{checked:true}).changed,false);
assert.equal(clicks,1); // Idempotent request must not toggle a second time.
element.click = ()=>{clicks++;};
assert.equal(run('set_checked',{checked:false}).matched,false); // Handler veto is reported, never retried.
assert.equal(clicks,2);
element.type='radio';
assert.throws(()=>run('set_checked',{checked:false}),/another radio/);
element.matches=()=>true;
assert.throws(()=>run('set_checked',{checked:true}),/disabled/);
element=Object.assign(new Select(),common,{tagName:'SELECT',current:'a',options:[{value:'a'},{value:'b'}]});
assert.equal(run('select',{value:'b'}).matched,true);
assert.deepEqual(events,['input','change']);
assert.equal(run('select',{value:'b'}).changed,false);
assert.equal(events.length,2);
assert.throws(()=>run('select',{value:'missing'}),/uniquely/);
element.options.push({value:'b'});
assert.throws(()=>run('select',{value:'b'}),/uniquely/);
element.options=[{value:'c',parentElement:{disabled:true}}];
assert.throws(()=>run('select',{value:'c'}),/enabled/);
element.multiple=true;
assert.throws(()=>run('select',{value:'c'}),/single select/);
console.log('Browser form contract tests passed');
// Snapshot option previews are explicit and share a budget across controls.
const selects = Array.from({length:100}, (_, i)=>({
  id:`select${i}`, tagName:'SELECT', type:'select-one',
  getClientRects:()=>[{}], getAttribute:()=>null,
  options:Array.from({length:100}, ()=>({value:'x'.repeat(300),label:'y'.repeat(300)}))
}));
Object.assign(context, {CSS:{escape:s=>s}, location:{href:'https://example.test/'}});
Object.assign(context.document, {title:'fixture',readyState:'complete',body:{innerText:''},forms:[],scripts:[],querySelectorAll:()=>selects});
const snapshot = context.blueDragonPage('snapshot',{});
const first = snapshot.elements[0];
assert.equal(first.options[0].value_truncated,true);
assert.equal(first.options[0].label_truncated,true);
assert.equal(first.options[0].value.length,200);
assert(first.options_omitted>0);
assert.equal(snapshot.elements.at(-1).options.length,0);
assert.equal(snapshot.elements.at(-1).options_omitted,100);
assert(JSON.stringify(snapshot).length<40000);
console.log('Snapshot option budget tests passed');

const readyDoc={readyState:'complete',location:{href:'https://example.test/app'},querySelectorAll:()=>[{innerText:'Loaded',getClientRects:()=>[{}]}]};
const readyContext=vm.createContext({window:{__bbStudioConsole:[]},document:readyDoc,console,setTimeout,Date});
vm.runInContext(readFileSync('browser-extension/shared/page.js','utf8'),readyContext);
const ready=await readyContext.blueDragonPage('wait_for',{selector:'#ready',text:'Loaded',timeout_ms:1000});
assert.equal(ready.matched,true);assert.equal(ready.react_complete_proven,false);
const absent=await readyContext.blueDragonPage('wait_for',{selector:'#ready',text:'Absent',timeout_ms:100});
assert.equal(absent.matched,false);
readyDoc.querySelectorAll=()=>[{tagName:'IFRAME',contentDocument:null}];
assert.throws(()=>readyContext.blueDragonPage('click',{selector:'#submit',frame_selector:'#private'}),/Cross-origin/);
const child={readyState:'complete',location:{href:'https://example.test/child'},querySelectorAll:()=>[{innerText:'Child ready',getClientRects:()=>[{}]}]};
readyDoc.querySelectorAll=()=>[{tagName:'IFRAME',contentDocument:child}];
const framed=await readyContext.blueDragonPage('wait_for',{frame_selector:'#child',selector:'#ready',text:'Child ready',timeout_ms:1000});
assert.equal(framed.matched,true);assert.equal(framed.url,'https://example.test/child');
console.log('Bounded readiness and same-origin/cross-origin frame contract tests passed');

// Producer/consumer contract: Rust reviews this exact collector output fixture.
const fields = Array.from({length:61}, (_,i)=>({id:`f${i}`,tagName:'INPUT',type:'hidden',name:i===60?'csrf_token':'ordinary',value:'PRIVATE_VALUE'}));
Object.assign(context.document,{querySelectorAll:()=>[], forms:Array.from({length:21},(_,i)=>({action:'https://example.test/submit',method:'post',elements:i===0?fields:[]}))});
const formSnapshot = JSON.parse(JSON.stringify(context.blueDragonPage('snapshot',{})));
assert.equal(formSnapshot.forms.length,20);
assert.equal(formSnapshot.forms_total,21);
assert.equal(formSnapshot.forms_omitted,1);
assert.equal(formSnapshot.forms_truncated,true);
assert.equal(formSnapshot.forms[0].fields.length,60);
assert.equal(formSnapshot.forms[0].fields_total,61);
assert.equal(formSnapshot.forms[0].fields_omitted,1);
assert.equal(formSnapshot.forms[0].fields_truncated,true);
assert(!JSON.stringify(formSnapshot).includes('csrf_token'));
assert(!JSON.stringify(formSnapshot).includes('PRIVATE_VALUE'));
const fixturePath='scripts/fixtures/browser-form-limits.json';
if(process.argv.includes('--write-fixture')) writeFileSync(fixturePath,JSON.stringify(formSnapshot)+'\n');
else assert.deepEqual(formSnapshot,JSON.parse(readFileSync(fixturePath,'utf8')));
console.log('Collector form/field limits fixture passed');

// Bounded scroll contract: exactly one operation, never scroll ancestors first.
let scrollCalls=0;
const scroller={scrollLeft:0,scrollTop:0,getClientRects:()=>[{}],scrollTo({left,top,behavior}){assert.equal(behavior,'instant');scrollCalls++;this.scrollLeft=Math.max(0,Math.min(300,left));this.scrollTop=Math.max(0,Math.min(500,top));}};
const scrollDoc={location:{href:'https://example.test/app'},scrollingElement:scroller,querySelectorAll:()=>[scroller]};
let scrollVisibility='visible';
const scrollContext=vm.createContext({window:{__bbStudioConsole:[],getComputedStyle:()=>({visibility:scrollVisibility,display:'block'})},document:scrollDoc,console});
vm.runInContext(readFileSync('browser-extension/shared/page.js','utf8'),scrollContext);
let moved=scrollContext.blueDragonPage('scroll',{delta_y:200});assert.equal(moved.after.y,200);assert.equal(moved.loading_complete_proven,false);
moved=scrollContext.blueDragonPage('scroll',{selector:'#box',delta_y:2000});assert.equal(moved.observed_delta.y,300);
moved=scrollContext.blueDragonPage('scroll',{delta_y:2000});assert.equal(moved.movement_observed,false);assert.equal(scrollCalls,3);
for(const args of [{},{delta_y:2001},{delta_y:0.1},{delta_y:'1'},{delta_y:1,selector:''}])assert.throws(()=>scrollContext.blueDragonPage('scroll',args));
assert.equal(scrollCalls,3);
scrollDoc.querySelectorAll=()=>[scroller,scroller];assert.throws(()=>scrollContext.blueDragonPage('scroll',{selector:'.ambiguous',delta_y:1}),/exactly one/);
scrollDoc.querySelectorAll=()=>[{tagName:'IFRAME',contentDocument:null}];assert.throws(()=>scrollContext.blueDragonPage('scroll',{frame_selector:'#private',delta_y:1}),/Cross-origin/);
assert.equal(scrollCalls,3);
console.log('Bounded scroll, clamping, no movement and negative contract tests passed');

scrollDoc.querySelectorAll=()=>[scroller];
for(const hidden of ['hidden','collapse']){
 scrollVisibility=hidden;
 assert.throws(()=>scrollContext.blueDragonPage('scroll',{selector:'#box',delta_y:100}),/CSS-visible/);
}
assert.equal(scrollCalls,3,'Hidden target must not receive a scroll');
console.log('CSS-hidden scroll target rejection passed');

// Click reports immediate UI change on a stable URL without replaying the action.
let heading = 'Closed';
let dispatched = 0;
const button = {...common, isConnected:true, getAttribute:()=>null,
  click(){ dispatched++; heading='Open'; }};
Object.assign(context.document, {
  scripts:[{src:'https://example.test/app.js',type:'module'}],
  querySelector:()=>({textContent:heading}),
  querySelectorAll:selector => selector === 'h1' ? [] : [button]
});
button.id='toggle';
button.tagName='BUTTON';
const receipt=run('click',{});
assert.deepEqual([...receipt.changed_fields],['heading']);
assert.equal(receipt.effect,'change_observed');
assert.equal(run('click',{}).effect,'unverified');
assert.equal(dispatched,2);
const compact=context.blueDragonPage('snapshot',{compact:true});
assert.equal(compact.script_tags[0].src,'https://example.test/app.js');
assert.equal(compact.script_tags[0].type,'module');
console.log('Same-URL click receipts and script tag identity passed');
