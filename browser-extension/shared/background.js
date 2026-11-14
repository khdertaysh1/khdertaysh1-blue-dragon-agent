'use strict';
const api = globalThis.browser || globalThis.chrome;
let socket = null, paired = false, currentTab = null, reconnect = null, serial = Promise.resolve();
const managed = new Set(), traffic = [];
const trim = value => String(value ?? '').slice(0, 2000);
function record(details, event) {
  if (!managed.has(details.tabId)) return;
  traffic.push({event, tab: details.tabId, request_id: details.requestId, method: details.method,
    url: trim(details.url), status: details.statusCode, type: details.type, error: details.error});
  if (traffic.length > 300) traffic.shift();
}
api.webRequest.onBeforeRequest.addListener(d => record(d,'request'), {urls:['http://*/*','https://*/*']});
api.webRequest.onCompleted.addListener(d => record(d,'response'), {urls:['http://*/*','https://*/*']});
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
async function execute(action,args) {
  if(action==='open') {
    const url=httpURL(args.url);
    if(currentTab===null || !managed.has(currentTab)) {const tab=await api.tabs.create({url:'about:blank',active:true});currentTab=tab.id;managed.add(tab.id);}
    await api.tabs.update(currentTab,{url});await readyTab(currentTab);return {opened:true};
  }
  if(currentTab===null || !managed.has(currentTab))throw Error('No managed tab. Use open first.');
  const tab=await api.tabs.get(currentTab);
  httpURL(tab.url);
  if(action==='navigate') {await api.tabs.update(currentTab,{url:httpURL(args.url)});await readyTab(currentTab);return {navigated:true};}
  if(action==='close') {await api.tabs.remove(currentTab);managed.delete(currentTab);currentTab=null;return {closed:true};}
  if(action==='network')return {entries:traffic.filter(e=>e.tab===currentTab),headers_and_cookies:'not captured',response_bodies:'not captured by extension'};
  if(action==='screenshot') {
    if(!tab.active)throw Error('Managed tab must be active for screenshot; activate it manually');
    return {data:await api.tabs.captureVisibleTab(tab.windowId,{format:'jpeg',quality:60})};
  }
  if(action==='source') {
    const url=httpURL(args.url);
    if(!traffic.some(e=>e.tab===currentTab&&e.url===url))throw Error('Source URL must have been observed in this managed tab');
    const response=await fetch(url,{credentials:'omit',redirect:'error',signal:AbortSignal.timeout(15000)});
    if(!response.body)throw Error('No response body');
    const reader=response.body.getReader();let chunks=[],size=0;
    try{while(true){const{value,done}=await reader.read();if(done)break;size+=value.length;if(size>128000)throw Error('Source exceeds 128KB');chunks.push(value);}}finally{await reader.cancel().catch(()=>{});}
    const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
    return {body:new TextDecoder().decode(bytes),status:response.status,note:'Fresh unauthenticated copy, not the original browser response body'};
  }
  if(!['snapshot','click','fill','evaluate','console'].includes(action))throw Error('Unsupported action');
  await readyTab(currentTab);
  const results=await api.scripting.executeScript({target:{tabId:currentTab},world:'MAIN',func:blueDragonPage,args:[action,args]});
  if(results[0]?.error)throw Error('Page execution failed');
  return results[0]?.result ?? null;
}
async function connect() {
  clearTimeout(reconnect);if(socket){socket.onclose=null;socket.close();}paired=false;
  const config=await api.storage.local.get(['token','port']);
  if(!/^[a-f0-9]{64}$/.test(config.token||'') || !Number.isInteger(config.port) || config.port<1 || config.port>65535)return;
  const ws=new WebSocket(`ws://127.0.0.1:${config.port}/bridge`);socket=ws;
  ws.onopen=()=>ws.send(JSON.stringify({type:'hello',version:1,token:config.token,capabilities:['open','navigate','snapshot','click','fill','evaluate','network','console','source','screenshot','close']}));
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
