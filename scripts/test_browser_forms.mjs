// Dependency-free contract tests for the shared CDP/extension page implementation.
import {readFileSync} from 'node:fs';
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
