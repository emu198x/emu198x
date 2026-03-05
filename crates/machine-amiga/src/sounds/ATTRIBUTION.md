# Sound Attribution

The floppy drive samples in this directory are extracted from:

**"Floppy disk drive - read"** by MrAuralization
https://freesound.org/people/MrAuralization/sounds/259292/
Licensed under Creative Commons Attribution 4.0 (CC BY 4.0)
https://creativecommons.org/licenses/by/4.0/

Recorded with a Zoom H1 at 24-bit/44.1 kHz, December 2014.

## Processing

The original 35-second field recording was processed to extract:

- `drive_click.raw` — Single step click (~150 ms), extracted from the
  first seek event at ~950 ms. Normalised, fade-out applied, resampled
  to 48 kHz mono 16-bit signed little-endian PCM.

- `drive_motor.raw` — Motor hum loop (~300 ms), extracted from the
  quiet motor-only section at ~200 ms before any head seeks begin.
  Normalised, crossfaded at loop points, resampled to 48 kHz mono
  16-bit signed little-endian PCM.
