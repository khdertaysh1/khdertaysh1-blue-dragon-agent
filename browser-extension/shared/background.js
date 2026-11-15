'use strict';
const api = globalThis.browser || globalThis.chrome;
let socket = null, paired = false, currentTab = null, reconnect = null, serial = Promise.resolve();
const managed = new Set(), traffic = [];
let ringEvictions = 0;
const trim = value => String(value ?? '').slice(0, 2000);
// Retain boolean indicators only: never serialize header values (including CSP nonces).
function policyIndicators(headers) {
  const available=Array.isArray(headers), selected=new Map();
  let complete=available && headers.length<=200;
  for(const header of available?headers.slice(0,200):[]) {
    if(typeof header.name!=='string'||header.name.length>80)continue;
    const key=header.name.toLowerCase();
    if(!['content-security-policy','content-security-policy-report-only','x-frame-options','x-content-type-options'].includes(key))continue;
    if(selected.has(key)){complete=false;continue;}
    const value=header.value;
    selected.set(key,'');
    if(typeof value!=='string' || value.length>2000 || new TextEncoder().encode(value).length>2000 || /[\r\n]/.test(value)){complete=false;continue;}
    selected.set(key,value);
  }
  const get=key=>selected.get(key)||'';
  const frame=value=>value.split(';').some(d=>d.trim().split(/\s+/)[0]?.toLowerCase()==='frame-ancestors');
  const xframe=get('x-frame-options').trim().toLowerCase();
  return {available,complete,x_frame_options_known_value_seen:['deny','sameorigin'].includes(xframe),
    csp_header_frame_directive_seen:frame(get('content-security-policy')),
    report_only_frame_directive_seen:frame(get('content-security-policy-report-only')),
    nosniff_value_seen:get('x-content-type-options').trim().toLowerCase()==='nosniff'};
}
function record(details, event) {
  if (!managed.has(details.tabId)) return;
  traffic.push({event, tab: details.tabId, request_id: trim(details.requestId), method: String(details.method||'').slice(0,16),
    url: trim(details.url), url_truncated:typeof details.url!=='string'||details.url.length>2000,
    ...(event==='response'?{security_header_indicators:policyIndicators(details.responseHeaders)}:{}), status: details.statusCode, type: String(details.type||'').slice(0,64), error: details.error===undefined?undefined:trim(details.error)});
  if (traffic.length > 300) {traffic.shift();ringEvictions++;}
}
api.webRequest.onBeforeRequest.addListener(d => record(d,'request'), {urls:['http://*/*','https://*/*']});
api.webRequest.onCompleted.addListener(d => record(d,'response'), {urls:['http://*/*','https://*/*']}, ['responseHeaders']);
api.webRequest.onErrorOccurred.addListener(d => record(d,'error'), {urls:['http://*/*','https://*/*']});
api.tabs.onRemoved.addListener(id => {managed.delete(id);if(currentTab===id)currentTab=null;});
api.action.onClicked.addListener(() => api.runtime.openOptionsPage());
function httpURL(value) {
  const url = new URL(value);
  if (!['http:','https:'].includes(url.protocol) || url.username || url.password) throw Error('HTTP(S) URL without credentials required');
  return url.href;
}
async function readyTab(id) {
  for(let i=0;i<150;i++) {const tab=await api.tabs.get(id);if(tab.status==='complete')return tab;await new Promise(r=>setTimeout(r,100));}
  throw Error('Navigation deadline reached; inspect before retry');
}
async function navigateTo(id,url) {
  let finish,fail;
  const done=new Promise((resolve,reject)=>{finish=resolve;fail=reject;});
  done.catch(()=>{});
  const listener=(tabId,change,tab)=>{
    if(tabId!==id || change.status!=='complete')return;
    if(url==='about:blank' ? tab.url==='about:blank' : /^https?:/.test(tab.url||''))finish(tab);
  };
  api.tabs.onUpdated.addListener(listener);
  const timer=setTimeout(()=>fail(Error('Navigation deadline reached; inspect before retry')),15000);
  try {
    const tab=await api.tabs.update(id,{url});
    if(tab.status==='complete' && tab.url===url)finish(tab);
    return await done;
  } finally {clearTimeout(timer);api.tabs.onUpdated.removeListener(listener);}
}
async function execute(action,args) {
  if(action==='open') {
    const url=httpURL(args.url);
    if(currentTab===null || !managed.has(currentTab)) {const tab=await api.tabs.create({url:'about:blank',active:true});currentTab=tab.id;managed.add(tab.id);}
    await navigateTo(currentTab,url);return {opened:true};
  }
  if(currentTab===null || !managed.has(currentTab))throw Error('No managed tab. Use open first.');
  const tab=await api.tabs.get(currentTab);
  httpURL(tab.url);
  if(action==='navigate') {const destination=httpURL(args.url);if(args.fresh_document===true)await navigateTo(currentTab,'about:blank');await navigateTo(currentTab,destination);return {navigated:true};}
  if(action==='close') {await api.tabs.remove(currentTab);managed.delete(currentTab);currentTab=null;return {closed:true};}
  if(action==='network') {
    const available=traffic.filter(e=>e.tab===currentTab), entries=[];let used=0;
    for(let i=available.length-1;i>=0;i--){const size=JSON.stringify(available[i]).length+1;if(used+size>180000)break;used+=size;entries.push(available[i]);}
    entries.reverse();
    return {entries,entries_omitted:available.length-entries.length,ring_entries_omitted:ringEvictions,
      ring_omission_scope:'all managed tabs since extension worker start',header_values_and_cookies:'not retained; bounded policy indicators only',response_bodies:'not captured by extension'};
  }
  if(action==='screenshot') {
    if(!tab.active)throw Error('Managed tab must be active for screenshot; activate it manually');
    return {data:await api.tabs.captureVisibleTab(tab.windowId,{format:'jpeg',quality:60})};
  }
  if(action==='source') {
    const url=httpURL(args.url);
    if(!traffic.some(e=>e.tab===currentTab&&!e.url_truncated&&e.url===url))throw Error('Source URL must have been observed in this managed tab');
    const response=await fetch(url,{credentials:'omit',redirect:'error',signal:AbortSignal.timeout(15000)});
    if(!response.body)throw Error('No response body');
    const reader=response.body.getReader();let chunks=[],size=0;
    try{while(true){const{value,done}=await reader.read();if(done)break;size+=value.length;if(size>128000)throw Error('Source exceeds 128KB');chunks.push(value);}}finally{await reader.cancel().catch(()=>{});}
    const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
    return {body:new TextDecoder().decode(bytes),status:response.status,note:'Fresh unauthenticated copy, not the original browser response body'};
  }
  if(!['snapshot','wait_for','click','fill','select','set_checked','scroll','evaluate','console'].includes(action))throw Error('Unsupported action');
  await readyTab(currentTab);
  const results=await api.scripting.executeScript({target:{tabId:currentTab},world:'MAIN',func:blueDragonPage,args:[action,args,true]});
  if(!Array.isArray(results) || results.length !== 1 || results[0]?.frameId !== 0 || results[0]?.error)throw Error('Page execution failed or returned no main-frame result');
  const envelope=results[0].result;
  if(!envelope || typeof envelope !== 'object' || Array.isArray(envelope) || envelope.blueDragonPageEnvelope !== 1 || typeof envelope.ok !== 'boolean')throw Error('Missing or invalid page execution envelope; inspect before retry');
  const keys=Object.keys(envelope).sort().join(',');
  if(envelope.ok) {
    if(keys !== 'blueDragonPageEnvelope,ok,value' || envelope.value === undefined)throw Error('Invalid successful page execution envelope');
    return envelope.value;
  }
  if(keys !== 'blueDragonPageEnvelope,error,ok' || typeof envelope.error !== 'string')throw Error('Invalid failed page execution envelope');
  throw Error(envelope.error.slice(0,300) || 'Page execution failed');
}
async function connect() {
  clearTimeout(reconnect);if(socket){socket.onclose=null;socket.close();}paired=false;
  const config=await api.storage.local.get(['token','port']);
  if(!/^[a-f0-9]{64}$/.test(config.token||'') || !Number.isInteger(config.port) || config.port<1 || config.port>65535)return;
  const ws=new WebSocket(`ws://127.0.0.1:${config.port}/bridge`);socket=ws;
  ws.onopen=()=>ws.send(JSON.stringify({type:'hello',version:1,token:config.token,capabilities:['open','navigate','snapshot','wait_for','click','fill','select','set_checked','scroll','evaluate','network','console','source','screenshot','close']}));
  ws.onmessage=event=> {
    if(typeof event.data!=='string'||event.data.length>24000)return;
    let packet;try{packet=JSON.parse(event.data);}catch{return;}
    if(packet.type==='ready'){paired=true;return;}
    if(!paired||packet.type!=='request'||!Number.isSafeInteger(packet.id))return;
    serial=serial.catch(()=>{}).then(async()=>{
      if(socket!==ws || ws.readyState!==WebSocket.OPEN)return;
      let response;
      try {
        const result=await execute(packet.action,packet.args||{});
        response={type:'response',id:packet.id,ok:true,result};
        const size=JSON.stringify(response).length;
        if(size>(packet.action==='screenshot'?2800000:240000))throw Error('Result exceeds limit');
      } catch(error) {response={type:'response',id:packet.id,ok:false,error:String(error.message||error).slice(0,300)};}
      // Never deliver a stale action response into a newly paired connection.
      if(socket===ws&&ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify(response));
    });
  };
  ws.onerror=()=>{};
  ws.onclose=()=>{if(socket===ws){paired=false;reconnect=setTimeout(connect,5000);}};
}
setInterval(()=>{if(socket?.readyState===WebSocket.OPEN)socket.send(JSON.stringify({type:'ping'}));},20000);
api.storage.onChanged.addListener((changes,area)=>{if(area==='local'&&(changes.token||changes.port))connect();});
connect();
