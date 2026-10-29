# SVI-328 pre-split v7 observations

Captured from 94564bea3 before moving production source. All ROM, cart and CAS
bytes are original synthetic inputs. A BIOS stub copies the guest into fixed
high RAM so it can enable both the lower-RAM overlay and cartridge bank. It
writes a lower-RAM sentinel, reads cartridge, keyboard and joystick sentinels,
paints patterned video, generates AY tone audio, and runs the cassette motor.
The guest records seeing both cassette input levels, not merely media presence.

Each state starts after 34,567 direct hardware T-states, partway through a
field. The runtime time counter retains its accepted legacy value after these
debugger steps. Three native field requests record every pixel, sample byte,
frame timestamp and the entire continued serialized state, including cassette
transport. Expectations must not be regenerated from migrated code. Capture
creates an exclusive new directory. No commercial firmware is included.
