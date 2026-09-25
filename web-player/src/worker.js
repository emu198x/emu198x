// Only the selected family is imported. Firmware is transferred from local file
// inputs, never fetched or sent to a server. All emulation runs in this worker.
let machine, kind;
const modules = {
  spectrum: ['spectrum', 'emu198x_spectrum_web'],
  c64: ['commodore', 'emu198x_commodore_web'],
  amiga: ['commodore', 'emu198x_commodore_web'],
  nes: ['nes', 'emu198x_nes_web'],
  'game-boy': ['game-boy', 'emu198x_game_boy_web'],
};
function audio() { return kind === 'spectrum' ? machine.audioDrain() : machine.audio(); }
function configure(rate) {
  if (kind === 'spectrum') { machine.configureAudio(rate, 2, rate / 2); machine.setAudioEnabled(true); }
  else machine.configure_audio(rate);
}
function load(format, bytes) {
  if (kind !== 'spectrum') return machine.load(format, bytes);
  if (format === 'sna' || format === 'z80') return machine.loadSnapshot(bytes, format);
  if (!['tap','tzx'].includes(format)) throw new Error('Choose a TAP, TZX, SNA or Z80 file.');
  machine.load('tape-1', 'tape', bytes); machine.autoload(250);
}
// Serialize async module loading and commands so a late boot cannot replace a
// newer machine. The page also replaces this entire worker on every restart.
let commands = Promise.resolve();
self.onmessage = ({data}) => { commands = commands.then(() => handle(data)); };
async function handle({id, command, args = []}) {
  try {
    let result;
    if (command === 'boot') {
      const [selected, roms, media, rate, variant] = args;
      if (!modules[selected] && !variant) throw new Error('Unknown machine');
      kind = selected;
      const [folder, name] = variant ? [variant.family, 'emu198x_fleet_web'] : modules[kind];
      if(variant)kind='fleet';
      const module = await import(`./modules/${folder}/${name}.js`); await module.default();
      if (kind === 'fleet') {
        machine=new module.Fleet(variant.family,variant.id);
        for(const [id,bytes] of Object.entries(roms))if(bytes.length)machine.firmware(id,bytes);
        machine.boot();
      }
      // A build with the verified 48K ROM embedded supplies it when the page
      // sends none; otherwise the visitor's own file is required, as before.
      else if (kind === 'spectrum') machine = roms.rom?.length || !module.Spectrum.createHeadlessBundled ? module.Spectrum.createHeadless(roms.rom) : module.Spectrum.createHeadlessBundled();
      else if (kind === 'c64') machine = module.Commodore.c64(roms.kernal,roms.basic,roms.chargen,roms.drive);
      else if (kind === 'amiga') machine = roms.extended?.length ? module.Commodore.amiga_aros(roms.kickstart,roms.extended) : module.Commodore.amiga(roms.kickstart);
      else if (kind === 'nes') machine = new module.Nes(media.bytes);
      else machine = new module.GameBoy('dmg',media.bytes);
      configure(rate);
      // C64 PRGs are loaded after the firmware reaches BASIC; importing before
      // its RAM test would erase the program. Disk-based machines mount first.
      if (media && !['nes','game-boy'].includes(kind)) {
        if ((kind === 'c64' || (kind==='fleet' && variant.family==='commodore-c64')) && media.format === 'prg') for (let i=0;i<150;i++) { machine.step(); audio(); }
        load(media.slot || media.format,media.bytes);
        if(media.autorun==='run' && (kind==='c64' || (kind==='fleet' && variant.family==='commodore-c64'))) {
          for(const key of ['R','U','N','enter']) {
            machine.key(key,true);for(let i=0;i<3;i++){machine.step();audio();}
            machine.key(key,false);for(let i=0;i<3;i++){machine.step();audio();}
          }
        }
      }
      result = {frameMs:machine.frame_ms(),keymap:kind==='fleet' ? JSON.parse(machine.keymap()) : {}};
    } else {
      if (!machine) throw new Error('Start a machine first');
      if (command === 'tick' || command === 'step') {
        const frames = command === 'step' ? (machine.step(),1) : kind === 'spectrum' ? machine.tick(args[0]) : machine.advance(args[0]);
        const pixels = frames ? (kind === 'spectrum' ? machine.frameRgba() : machine.pixels()) : new Uint8Array();
        const samples = audio();
        const [width,height] = kind === 'spectrum' ? machine.frameSize() : [machine.width(),machine.height()];
        self.postMessage({id,result:{frames,pixels,audio:samples,width,height}},[pixels.buffer,samples.buffer]); return;
      }
      if (command === 'input') for (const [type,...values] of args[0]) {
        if (type === 'code') (values[1] ? machine.keyDown(values[0]) : machine.keyUp(values[0]));
        else if (type === 'key') machine.key(...values);
        else if (type === 'button') machine.button(...values);
        else if (type === 'move') machine.mouse_move(...values);
        else if (type === 'mouse') machine.mouse_button(...values);
        else throw new Error('Unknown input');
      }
      else if (command === 'load') load(...args);
      else if (command === 'transport' && kind==='fleet') machine.transport(...args);
      else if (command === 'save') { machine.step(); audio(); result=machine.save_state(); }
      else if (command === 'restore') machine.restore_state(args[0]);
      else if (command === 'exportMedia') result=machine.export_media(args[0]);
      else if (command === 'audioRate') configure(args[0]);
      else if (!['tick','step'].includes(command)) throw new Error('Unknown player command');
    }
    self.postMessage({id,result});
  } catch (error) { self.postMessage({id,error:String(error)}); }
}
