// Contract fixture only, not a real browser.
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const h={id:'heading',tagName:'H1',innerText:'Reached'};
const link={id:'next',tagName:'A',innerText:'Next',href:'http://127.0.0.1/done'};
const context=vm.createContext({window:{__bbStudioConsole:[]},CSS:{escape:s=>s},location:{href:'http://127.0.0.1/done'},document:{title:'After',readyState:'complete',body:{innerText:'Reached '.repeat(1000)},querySelectorAll:s=>s==='h1'?[h]:[link]}});
vm.runInContext(readFileSync('browser-extension/shared/page.js','utf8'),context);
const result=context.blueDragonPage('snapshot',{compact:true,selector:'h1'});
assert.equal(result.url,'http://127.0.0.1/done');assert.equal(result.title,'After');
assert.deepEqual(Array.from(result.h1),['Reached']);assert.equal(result.elements[0].selector,'#heading');
assert.equal(result.text.length,2000);assert.equal(result.observation_kind,'current_page_read');assert.equal(result.untrusted_web_content,true);
console.log('Compact DOM contract passed; real browser not exercised');
