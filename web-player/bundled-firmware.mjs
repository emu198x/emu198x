// The one firmware image the player may embed: the Sinclair 48K ROM, under
// Amstrad's permission for emulator authors. It is compiled into the Spectrum
// module's wasm, never copied into the distribution as a file, and only the
// genuine image is accepted, because the permission requires the copyright
// messages to be unaltered. A patched 48K ROM compiles and boots perfectly
// well, so size is not enough: the bytes must be the published ones.
// See knowledge/decisions/test-rom-policy.md
// § Firmware in a published browser build.
import {createHash} from 'node:crypto';
import {existsSync,readFileSync} from 'node:fs';
export const SPECTRUM_48K_ROM={size:16384,sha1:'5ea7c2b824672e914525d1d5c419d71b84a426a2',crc32:'ddee531f'};
// Returns the path to embed, or null when the variable is unset or empty (a
// CI run without the secret builds the bring-your-own-firmware player).
export function spectrum48kRom(env=process.env) {
 const file=env.EMU198X_SPECTRUM_48K_ROM;
 if(!file)return null;
 if(!existsSync(file))throw new Error('EMU198X_SPECTRUM_48K_ROM is set but names no file; refusing to build without the firmware it asks for.');
 const bytes=readFileSync(file);
 if(bytes.length!==SPECTRUM_48K_ROM.size)throw new Error(`EMU198X_SPECTRUM_48K_ROM is ${bytes.length} bytes; the Sinclair 48K ROM is ${SPECTRUM_48K_ROM.size}. Refusing to bundle it.`);
 const sha1=createHash('sha1').update(bytes).digest('hex');
 if(sha1!==SPECTRUM_48K_ROM.sha1)throw new Error(`EMU198X_SPECTRUM_48K_ROM has SHA1 ${sha1}, not the unmodified Sinclair 48K ROM (${SPECTRUM_48K_ROM.sha1}, CRC32 ${SPECTRUM_48K_ROM.crc32}). Only the verbatim image may be redistributed; refusing to bundle a patched or different ROM.`);
 return file;
}
