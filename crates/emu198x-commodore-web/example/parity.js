// Shared Node/browser checker. It deliberately uses only the public binding.
export function runCheck(Commodore, kind, roms, expected) {
  const check = (condition, message) => { if (!condition) throw new Error(message); };
  let machine;
  const create = () => kind === 'c64'
    ? Commodore.c64(...roms.map(bytes=>new Uint8Array(bytes)))
    : Commodore.amiga(new Uint8Array(roms[0]));
  for (let repeat=0; repeat<2; repeat++) {
    machine = create(); machine.configure_audio(48000);
    let samples=0, energy=0;
    try {
      check(machine.advance(NaN)===0,'NaN clock input must not advance');
      check(machine.advance(-10)===0,'Negative clock input must not advance');
      let rejected=false;
      try { machine.load('invalid',new Uint8Array([0])); } catch { rejected=true; }
      check(rejected,'Malformed media was accepted');
      for(let frame=1;frame<=360;frame++) {
        if(frame===301) {machine.key('a',true);machine.joystick('fire',true);machine.mouse_move(20,-10);machine.mouse_button('left',true);}
        if(frame===311) {machine.key('a',false);machine.joystick('fire',false);machine.mouse_button('left',false);}
        machine.step();
        const audio=machine.audio(); samples+=audio.length;
        for(const value of audio) {check(Number.isFinite(value),'Non-finite audio');energy+=value*value;}
        const target=expected.checkpoints.find(x=>x.frame===frame);
        if(!target)continue;
        let hash=2166136261;
        for(const byte of machine.pixels())hash=Math.imul(hash^byte,16777619)>>>0;
        check(hash.toString(16).padStart(8,'0')===target.hash,`Frame ${frame}: pixel mismatch`);
        check(machine.width()===target.width && machine.height()===target.height,`Frame ${frame}: dimensions differ`);
        check(samples===target.samples,`Frame ${frame}: sample count ${samples} != ${target.samples}`);
        // Floating-point filters need not be bit-identical across native and WASM.
        check(Math.abs(energy-target.energy)<=Math.max(1e-6,Math.abs(target.energy)*1e-5),`Frame ${frame}: audio energy differs`);
      }
    } finally {machine.free();}
  }
  let rejected=false;
  try {Commodore.amiga(new Uint8Array(1));} catch {rejected=true;}
  check(rejected,'Invalid Kickstart accepted');
  return `${kind}: native/WASM frame, sample-count and audio-energy parity at 1/100/300/310/360 frames; two fresh boots`;
}
