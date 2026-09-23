import {readPreferences} from './preferences.js';
// A versioned embed must also load its matching host, rather than an older
// cached host with a different public API.
const hostUrl=new URL('./player.js',import.meta.url);
hostUrl.search=new URL(import.meta.url).search;
const {mountPlayer}=await import(hostUrl.href);
const asset = name => new URL(name, import.meta.url);
// The template and catalogue are shared by all instances; WASM is still loaded
// only by the worker when the visitor chooses software or starts a computer.
let resources;
async function loadResources() {
  resources ??= Promise.all(['template.html','catalog.json','player.css'].map(async name => {
    const response=await fetch(asset(name));
    if(!response.ok)throw new Error('The player could not be loaded. Reload this page to try again.');
    return name.endsWith('.json') ? response.json() : response.text();
  })).catch(error=>{resources=undefined;throw error;});
  return resources;
}
class EmulatorPlayer extends HTMLElement {
  connectedCallback() {
    const connection = this.connection = Symbol();
    let root=this.shadowRoot;
    this.setAttribute('aria-busy','true');
    this.ready=loadResources().then(([template,catalog,css])=>{
      if(this.connection!==connection || !this.isConnected)return;
      const id=this.getAttribute('system');
      const system=catalog.find(entry=>entry.id===id || entry.aliases.includes(id));
      if(!system)throw new Error('This machine is not available in the browser player.');
      root ||= this.attachShadow({mode:'open'});
      const render=selectedVariant=>{
        this.cleanup?.();
        root.innerHTML=template;
        const style=document.createElement('style'); style.textContent=css; root.prepend(style);
        const src=this.getAttribute('src');
        const example=src?{url:src,title:this.getAttribute('example-title') || 'Lesson example'}:null;
        const externalSource=this.hasAttribute('external-source');
        this.cleanup=mountPlayer(root,{...system,selectedVariant,example,externalSource,demo:externalSource?undefined:system.demo},asset,render);
      };
      const preferred=readPreferences(system.id).variant;
      render(this.getAttribute('variant') || system.variantAliases?.[id] || (system.variants.some(v=>v.id===preferred)?preferred:undefined));
      this.removeAttribute('aria-busy');
    }).catch(error=>{
      if(this.connection!==connection)return;
      root ||= this.attachShadow({mode:'open'});
      root.replaceChildren(); const message=document.createElement('p'); message.setAttribute('role','alert'); message.textContent=error.message; root.append(message); const link=document.createElement('a');link.href='https://emu198x.github.io/downloads/';link.textContent='Download Emu198x for desktop';root.append(link);
      this.removeAttribute('aria-busy');
    });
  }
  pause() { this.cleanup?.pause(); }
  async loadMedia(program) {
    await this.ready;
    if(!this.isConnected || !this.cleanup?.loadMedia)throw new Error('The player is not ready. Reload this page to try again.');
    return this.cleanup.loadMedia(program);
  }
  disconnectedCallback() { this.connection=undefined; this.cleanup?.(); this.cleanup=undefined; }
}
if(!customElements.get('emu198x-player'))customElements.define('emu198x-player',EmulatorPlayer);
