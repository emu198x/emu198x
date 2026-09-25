#!/usr/bin/env node
// Shared distribution consumed by both public sites. Firmware-free unless
// EMU198X_SPECTRUM_48K_ROM names the genuine Sinclair 48K ROM, which is then
// compiled into the Spectrum module's wasm (never copied as a file).
import {execFileSync} from 'node:child_process';
import {cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {stageDemos} from '../web-player/stage-demos.mjs';
import {makeCatalogue} from '../web-player/catalogue.mjs';
import {spectrum48kRom} from '../web-player/bundled-firmware.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const output=process.argv[2];
if(!output)throw new Error('Pass an output directory (for example public/emulators).');
const destination=path.resolve(output);
if(!['emulators','browser-player'].includes(path.basename(destination)))throw new Error('Output directory must be named emulators or browser-player.');
const families=['spectrum','commodore','nes','game-boy'];
// Verified before anything is built, so a wrong image fails fast and loudly.
const spectrumRom=spectrum48kRom();
const bundledSpectrum48k=Boolean(spectrumRom);
// Only the legacy Spectrum crate serves the 48K model; its `bundled-rom`
// feature embeds the image. Every other module stays bring-your-own.
const bundleEnv=bundledSpectrum48k?{...process.env,EMU198X_SPECTRUM_48K_ROM:path.resolve(spectrumRom)}:process.env;
if(bundledSpectrum48k)console.log('Browser player: embedding the verified Sinclair 48K ROM in the Spectrum module.');
const buildRoot=path.join(root,'target'); mkdirSync(buildRoot,{recursive:true});
const stage=mkdtempSync(path.join(buildRoot,'browser-player-'));
try {
  cpSync(path.join(root,'web-player/src'),stage,{recursive:true});
  stageDemos(stage);
  // These are shared with the existing Commodore prototype, so its regression
  // checks also protect the player. Do not maintain a second copy of either.
  for(const file of ['audio.js','mouse.js'])cpSync(path.join(root,'crates/emu198x-commodore-web/example',file),path.join(stage,file));
  if(process.env.WASM_BINDGEN)execFileSync('cargo',['build','--locked','--release','--target','wasm32-unknown-unknown',...families.flatMap(f=>['-p',`emu198x-${f}-web`]),...(bundledSpectrum48k?['--features','emu198x-spectrum-web/bundled-rom']:[])],{cwd:root,stdio:'inherit',env:bundleEnv});
  for(const family of families) {
    const crate=`emu198x-${family}-web`, name=crate.replaceAll('-','_'), out=path.join(stage,'modules',family);
    if(process.env.WASM_BINDGEN) {
      const target=path.resolve(root,process.env.CARGO_TARGET_DIR || 'target');
      execFileSync(process.env.WASM_BINDGEN,[path.join(target,'wasm32-unknown-unknown/release',`${name}.wasm`),'--target','web','--out-dir',out],{stdio:'inherit'});
    } else execFileSync('wasm-pack',['build',path.join(root,'crates',crate),'--target','web','--release','--out-dir',out,'--no-pack','--no-opt',...(bundledSpectrum48k && family==='spectrum'?['--','--features','bundled-rom']:[])],{cwd:root,stdio:'inherit',env:bundleEnv});
    // Copy only runtime assets, never package metadata or local firmware.
    for(const file of [`${name}.d.ts`,`${name}_bg.wasm.d.ts`,'package.json','.gitignore'])rmSync(path.join(out,file),{force:true});
  }
  const profiles=JSON.parse(execFileSync('cargo',['run','--locked','--quiet','-p','emu198x-fleet-web','--features','all-families','--bin','fleet-catalogue'],{cwd:root,encoding:'utf8',maxBuffer:4*1024*1024}));
  const catalogue=makeCatalogue(profiles,{bundledSpectrum48k});
  writeFileSync(path.join(stage,'catalog.json'),JSON.stringify(catalogue,null,2)+'\n');
  const fleetFamilies=catalogue.map(entry=>entry.family);
  for(const family of fleetFamilies) {
    const out=path.join(stage,'modules',family), name='emu198x_fleet_web';
    if(process.env.WASM_BINDGEN) {
      execFileSync('cargo',['build','--locked','--release','--lib','-p','emu198x-fleet-web','--no-default-features','--features',family,'--target','wasm32-unknown-unknown'],{cwd:root,stdio:'inherit'});
      const target=path.resolve(root,process.env.CARGO_TARGET_DIR || 'target');
      execFileSync(process.env.WASM_BINDGEN,[path.join(target,'wasm32-unknown-unknown/release',`${name}.wasm`),'--target','web','--out-dir',out],{stdio:'inherit'});
    } else execFileSync('wasm-pack',['build',path.join(root,'crates/emu198x-fleet-web'),'--target','web','--release','--out-dir',out,'--no-pack','--no-opt','--','--no-default-features','--features',family],{cwd:root,stdio:'inherit'});
    for(const file of [`${name}.d.ts`,`${name}_bg.wasm.d.ts`,'package.json','.gitignore'])rmSync(path.join(out,file),{force:true});
  }
  cpSync(path.join(root,'LICENSE'),path.join(stage,'LICENSE.txt'));
  execFileSync(process.execPath,[path.join(root,'web-player/check-save-file.mjs')],{cwd:root,stdio:'inherit'});
  execFileSync(process.execPath,[path.join(root,'web-player/check-bundled-firmware.mjs'),stage],{cwd:root,stdio:'inherit'});
  execFileSync(process.execPath,[path.join(root,'web-player/check-worker.mjs'),stage],{cwd:root,stdio:'inherit'});
  execFileSync(process.execPath,[path.join(root,'web-player/check-fleet.mjs'),stage],{cwd:root,stdio:'inherit'});
  const revision=execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim();
  const modified=execFileSync('git',['status','--porcelain','--untracked-files=normal'],{cwd:root,encoding:'utf8'}).trim().length>0;
  writeFileSync(path.join(stage,'build.json'),JSON.stringify({revision,modified,families,fleetFamilies,bundledSpectrum48kFirmware:bundledSpectrum48k},null,2)+'\n');
  // Replace only after all family builds have succeeded.
  rmSync(destination,{recursive:true,force:true}); mkdirSync(path.dirname(destination),{recursive:true}); cpSync(stage,destination,{recursive:true});
  console.log(`Browser player: ${destination}`);
} finally { rmSync(stage,{recursive:true,force:true}); }
