# ColecoVision pre-split v5 observations

Captured before moving production source at revision 94564bea3. All ROM bytes
are an original synthetic guest; no commercial firmware or cartridge is used.
The guest paints patterned text, produces tone audio, reads both controllers
in joystick/keypad modes and acknowledges VDP NMI through a preserving handler.
Each state starts after 12,345 direct hardware steps, partway through a field.
The runtime time counter retains its accepted legacy value after debugger steps.
Three native field requests record time, every ARGB32 pixel, every f32 sample
and the complete final serialized state. Expectations must not be regenerated
from migrated code. The capture command creates an exclusive new directory.
