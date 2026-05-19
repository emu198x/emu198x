# Audio Mixing

Each system combines multiple audio sources into a final output. The emulator uses integer arithmetic and Bresenham downsampling — no floating-point in the audio path.

## Spectrum audio chain

1. **Beeper**: port $FE bit 4. Area-averaging accumulator in `BeeperAudio`
2. **Tape EAR**: mixed into beeper level
3. **AY-3-8912** (128K+): Bresenham downsampling at 44.1 kHz via internal /8 prescaler
4. **Mix**: 30% beeper, 70% AY, through single-pole RC low-pass filter (~10 kHz cutoff)
5. **Stereo**: ACB panning (A=left, C=right, B=centre) for 128K+
6. **Output**: `blip_buf` crate for band-limited synthesis

## EAR/MIC feedback

Port $FE bit 6 reads back differently depending on tape state:
- **Tape connected**: tape signal drives bit 6 (high=0, low=1)
- **No tape**: beeper/MIC output feeds back to bit 6

Must suppress feedback when tape is connected, or tape loading fails.
