import init, { Commodore } from '../pkg/emu198x_commodore_web.js';
let machine;
const ready = init();
self.onmessage = async ({ data: { id, command, args = [] } }) => {
  try {
    await ready;
    let result;
    if (command === 'boot') {
      machine?.free();
      const [kind, roms, rate] = args;
      machine = kind === 'c64'
        ? Commodore.c64(roms.kernal, roms.basic, roms.chargen, roms.drive)
        : roms.extended
          ? Commodore.amiga_aros(roms.kickstart, roms.extended)
          : Commodore.amiga(roms.kickstart);
      machine.configure_audio(rate);
      result = { frameMs: machine.frame_ms() };
    } else {
      if (!machine) throw new Error('Boot a machine first');
      if (command === 'tick' || command === 'step') {
        const start = performance.now();
        const frames = command === 'step' ? (machine.step(), 1) : machine.advance(args[0]);
        const elapsed = performance.now() - start;
        const pixels = frames ? machine.pixels() : new Uint8Array();
        const audio = machine.audio();
        self.postMessage({ id, result: { frames, elapsed, pixels, audio,
          width: machine.width(), height: machine.height() } }, [pixels.buffer, audio.buffer]);
        return;
      } else if (command === 'benchmark') {
        // Bounded and intentionally excludes presentation/transfer time.
        const times = [];
        for (let i = 0; i < 120; i++) {
          const start = performance.now(); machine.step(); machine.audio();
          times.push(performance.now() - start);
        }
        times.sort((a, b) => a - b);
        result = { mean: times.reduce((a, b) => a + b, 0) / times.length,
          p95: times[Math.floor(times.length * .95)], frameMs: machine.frame_ms() };
      } else if (command === 'load') {
        machine.load(...args);
      } else if (command === 'audioRate') {
        machine.configure_audio(args[0]);
      } else if (command === 'input') {
        for (const [type, ...values] of args[0]) {
          if (type === 'key') machine.key(...values);
          else if (type === 'joystick') machine.joystick(...values);
          else if (type === 'move') machine.mouse_move(...values);
          else if (type === 'mouse') machine.mouse_button(...values);
          else throw new Error(`Unknown input: ${type}`);
        }
      } else throw new Error(`Unknown command: ${command}`);
    }
    self.postMessage({ id, result });
  } catch (error) { self.postMessage({ id, error: String(error) }); }
};
