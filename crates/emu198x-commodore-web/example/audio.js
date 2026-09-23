// Bounded stereo ring. Rebuffer after a gap; never accumulate seconds of latency.
class EmulatorAudio extends AudioWorkletProcessor {
  constructor() {
    super();
    this.capacity = Math.ceil(sampleRate / 4) * 2;
    this.ring = new Float32Array(this.capacity);
    this.read = 0; this.length = 0; this.playing = false;
    this.port.onmessage = ({data}) => {
      if (data === 'clear') { this.length = 0; this.read = 0; this.playing = false; return; }
      for (const sample of data) {
        if (this.length === this.capacity) { this.read = (this.read + 1) % this.capacity; this.length--; }
        this.ring[(this.read + this.length) % this.capacity] = sample;
        this.length++;
      }
    };
  }
  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    if (!this.playing && this.length >= sampleRate * .08 * 2) this.playing = true;
    for (let i = 0; i < left.length; i++) {
      if (this.playing && this.length >= 2) {
        left[i] = this.ring[this.read]; this.read = (this.read + 1) % this.capacity;
        right[i] = this.ring[this.read]; this.read = (this.read + 1) % this.capacity;
        this.length -= 2;
      } else { this.playing = false; left[i] = right[i] = 0; }
    }
    return true;
  }
}
registerProcessor('emulator-audio', EmulatorAudio);
