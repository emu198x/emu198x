// Versioned, bounded binary container. ROM/media bytes stay binary rather than
// expanding through base64. SHA-256 detects truncated or corrupted downloads.
const magic=new TextEncoder().encode('EMU198X1');
const MAX=256*1024*1024;
const digest=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');
export async function encodeSave(save) {
 const parts=[];let length=0;
 const part=bytes=>{if(!(bytes instanceof Uint8Array))throw new Error('Invalid save data.');const entry={offset:length,length:bytes.length};parts.push(bytes);length+=bytes.length;return entry;};
 const [kind,roms,media,rate,variant]=save.boot;
 const header={version:1,system:save.system,variant:save.variant,date:save.date,mounted:save.mounted,
  boot:[kind,Object.fromEntries(Object.entries(roms).map(([id,bytes])=>[id,part(bytes)])),media?{...media,bytes:part(media.bytes)}:null,rate,variant],state:part(save.state)};
 if(length>MAX)throw new Error('This save exceeds the 256 MiB file limit.');
 const payload=await new Blob(parts).arrayBuffer();header.sha256=await digest(payload);
 const metadata=new TextEncoder().encode(JSON.stringify(header));
 if(metadata.length>65536)throw new Error('Save-file metadata is too large.');
 const prefix=new Uint8Array(12);prefix.set(magic);new DataView(prefix.buffer).setUint32(8,metadata.length,true);
 return new Blob([prefix,metadata,payload],{type:'application/octet-stream'});
}
export async function decodeSave(file,system,variant) {
 if(file.size>MAX+65548 || file.size<12)throw new Error('Invalid or oversized save file.');
 const bytes=new Uint8Array(await file.arrayBuffer());
 if(!magic.every((b,i)=>bytes[i]===b))throw new Error('Choose an Emu198x save file (.emu198x).');
 const length=new DataView(bytes.buffer).getUint32(8,true);
 if(length>65536 || 12+length>bytes.length)throw new Error('Invalid save-file header.');
 let header;try{header=JSON.parse(new TextDecoder().decode(bytes.subarray(12,12+length)));}catch{throw new Error('Invalid save-file header.');}
 if(header.version!==1 || header.system!==system || header.variant!==variant)throw new Error('This save belongs to another system or model. Select its model before importing.');
 const payload=bytes.subarray(12+length);
 if(await digest(payload)!==header.sha256)throw new Error('The save file is damaged or incomplete.');
 const part=entry=>{if(!entry || !Number.isSafeInteger(entry.offset) || !Number.isSafeInteger(entry.length) || entry.offset<0 || entry.length<0 || entry.offset+entry.length>payload.length)throw new Error('Invalid save-file data.');return payload.slice(entry.offset,entry.offset+entry.length);};
 if(!Array.isArray(header.boot) || header.boot.length!==5 || !header.boot[1] || typeof header.boot[1]!=='object')throw new Error('Invalid saved machine.');
 const [kind,roms,media,rate,model]=header.boot;
 return {...header,boot:[kind,Object.fromEntries(Object.entries(roms).map(([id,value])=>[id,part(value)])),media?{...media,bytes:part(media.bytes)}:null,rate,model],state:part(header.state)};
}
