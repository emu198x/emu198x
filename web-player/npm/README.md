# @emu198x/player

The Emu198x browser player: every runtime family's WebAssembly module, the machine catalogue, the demo programs, and the page scripts that run them in a worker. This is the same distribution the Code198x and Emu198x sites embed.

It's built once, by the Emu198x release workflow, and published at the Emu198x release's version, so `@emu198x/player@0.38.4` is the player from Emu198x v0.38.4. `build.json` names the exact commit.

## Using it

Copy the package's files to your site, under a directory named `emulators`, and serve them from your own origin:

```sh
cp -R node_modules/@emu198x/player/. public/emulators/
```

The modules load relative to that directory. Firmware and media are chosen by the reader and stay on their device; nothing is uploaded.

## Firmware

One image is included: the Sinclair ZX Spectrum 48K ROM, compiled into the Spectrum module's wasm and never served as a separate file. Amstrad permits its distribution with emulators, on the condition that the copyright messages stay unaltered and the following acknowledgement is shown:

> Amstrad have kindly given their permission for the redistribution of their copyrighted material but retain that copyright.

Every other model and system is bring-your-own firmware. The permission's scope is recorded in Emu198x's `knowledge/decisions/test-rom-policy.md`.

## Licence

The emulator code is GPL-2.0-or-later; see `LICENSE.txt`. The Spectrum 48K ROM remains Amstrad's copyright and is distributed under the permission above.
