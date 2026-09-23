// Origin-local storage only. Writes are explicit user actions; no network calls.
const database='emu198x-player-v1';
function open() {
 return new Promise((resolve,reject)=>{
  if(!globalThis.indexedDB){reject(new Error('Device storage is unavailable. You can still export a save file.'));return;}
  const request=indexedDB.open(database,1);
  request.onupgradeneeded=()=>{request.result.createObjectStore('firmware');request.result.createObjectStore('saves');};
  request.onsuccess=()=>resolve(request.result);
  request.onerror=()=>reject(new Error('Device storage is unavailable. You can still export a save file.'));
  request.onblocked=()=>reject(new Error('Close other player tabs and retry device storage.'));
 });
}
async function operation(store,key,value,mode) {
 const db=await open();
 try {return await new Promise((resolve,reject)=>{
  const transaction=db.transaction(store,mode==='get'?'readonly':'readwrite');
  const objectStore=transaction.objectStore(store);
  const request=mode==='get'?objectStore.get(key):mode==='delete'?objectStore.delete(key):objectStore.put(value,key);
  transaction.oncomplete=()=>resolve(request.result);
  transaction.onabort=transaction.onerror=()=>reject(new Error('Could not update device storage. It may be full or disabled; export a save file instead.'));
 });}finally{db.close();}
}
export const deviceStore={get:(store,key)=>operation(store,key,undefined,'get'),put:(store,key,value)=>operation(store,key,value,'put'),delete:(store,key)=>operation(store,key,undefined,'delete')};
