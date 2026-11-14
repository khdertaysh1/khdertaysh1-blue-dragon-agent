const api=globalThis.browser||globalThis.chrome;
document.getElementById('pair').addEventListener('submit',async event=>{
  event.preventDefault();const token=document.getElementById('token').value.trim(),port=Number(document.getElementById('port').value);
  if(!/^[a-f0-9]{64}$/.test(token)||!Number.isInteger(port)||port<1||port>65535){document.getElementById('status').textContent='Invalid token or port';return;}
  await api.storage.local.set({token,port});document.getElementById('status').textContent='Saved. Check /browser status in Blue Dragon for connection readiness.';
});
