#!/usr/bin/env python3
"""Package production-decoder validation images without changing the images."""
from pathlib import Path
import hashlib
import html
import json
import re
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
out = Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'target/signal-live-validation'
log = (out / 'validation.log').read_text()
source_paths = [
    'crates/emu198x-native-video/src/signal.rs',
    'crates/emu198x-native-video/src/signal.wgsl',
    'crates/emu198x-native-video/src/lib.rs',
    'crates/emu198x-shell/src/host.rs',
    'crates/runtime-sinclair-zx-spectrum/src/signal.rs',
    'crates/runtime-sinclair-zx-spectrum/src/runtime.rs',
    'crates/runtime-commodore-c64/src/signal.rs',
    'crates/runtime-commodore-c64/src/runtime.rs',
    'crates/runtime-nintendo-nes/src/signal.rs',
    'crates/runtime-nintendo-nes/src/runtime.rs',
    'crates/runtime-commodore-amiga/src/runtime.rs',
    'tools/spectrum-composite/src/validate_live.rs',
]
metrics = {
    'adapter': re.search(r'^adapter: (.+)$', log, re.M).group(1),
    'decode_median_ms': {name: float(value) for name, value in re.findall(r'^(.+): ([0-9.]+) ms$', log, re.M)},
    'spectrum_max_byte_deltas': [int(value) for value in re.findall(r'max byte delta (\d+)', log)],
    'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
    'source_sha256': {path: hashlib.sha256((root/path).read_bytes()).hexdigest() for path in source_paths},
    'image_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.png')},
    'timing_scope': '30 warmed completed decodes; uploads + four GPU passes + completion; no emulator execution, setup, readback, CRT or window presentation',
}
assert len(metrics['spectrum_max_byte_deltas']) == 4
assert max(metrics['spectrum_max_byte_deltas']) <= 1
(out/'results.json').write_text(json.dumps(metrics, indent=2)+'\n')

cases = [
    ('Spectrum 48K', 'spectrum-raw.png', 'spectrum-signal.png', 'Ferranti pin-level model → PAL composite receiver. RF is not modelled.'),
    ('C64 · composite', 'c64-raw.png', 'c64-signal.png', 'VIC colour codes → sampled composite → PAL receiver.'),
    ('C64 · monitor', 'c64-signal.png', 'c64-monitor.png', 'Left: composite. Right: separate luma/chroma. Both use the same source levels.'),
    ('NES NTSC', 'nes-raw.png', 'nes-signal.png', 'Project-owned cartridge. Per-pixel colour and emphasis → 2C02 waveform → NTSC receiver.'),
    ('Amiga RGB test plate', 'amiga-rgb-raw.png', 'amiga-rgb-monitor.png', 'Deterministic RGB plate at the real runtime geometry and clock → nominal 5 MHz RGB monitor.'),
]
sections = []
for name, before, after, caption in cases:
    assert (out/before).is_file() and (out/after).is_file()
    sections.append(f'''<section><h2>{html.escape(name)}</h2><p>{html.escape(caption)}</p>
<div class="compare"><img src="{before}" alt="Before source decoding"><div class="after"><img src="{after}" alt="After receiver decoding"></div></div>
<input type="range" min="0" max="100" value="50" aria-label="Reveal receiver output" oninput="this.previousElementSibling.querySelector('.after').style.clipPath='inset(0 '+(100-this.value)+'% 0 0)'">
<p class="legend">Left image: receiver output · right image: raw source (C64 monitor comparison uses composite as baseline).</p></section>''')
rows = ''.join(f'<tr><td>{html.escape(name)}</td><td>{value:.3f} ms</td></tr>' for name,value in metrics['decode_median_ms'].items())
page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Four-machine live signal experiment</title>
<style>body{font:17px/1.5 system-ui,sans-serif;max-width:1000px;margin:40px auto;padding:0 24px;background:#11141a;color:#edf0f5}h1,h2{line-height:1.2}p{max-width:850px}section{margin:48px 0}a{color:#9dc4ff}.compare{position:relative;max-width:100%;width:100%;background:#000;overflow:hidden}.compare>img{display:block;width:100%;image-rendering:pixelated}.after{position:absolute;inset:0;clip-path:inset(0 50% 0 0)}.after img{width:100%;height:100%;image-rendering:pixelated}input{width:100%}.legend{font-size:14px;color:#abb6c6}table{border-collapse:collapse}td,th{padding:9px 20px;text-align:left;border-bottom:1px solid #364052}code{color:#b7d0f7}</style>
<h1>Four-machine live signal experiment</h1><p>The production receiver now accepts electrical frames from Spectrum, C64, NES and Amiga runtimes. These comparisons show source decoding <em>before</em> the shared CRT presentation stage. The actual native windows feed this decoded texture directly into either CRT presentation or a sharp modern display.</p>
<p>Use <code>--video signal</code> for the electrical source and <code>--video monitor</code> for an available direct monitor connection. Spectrum currently covers the 16K/48K/+ timing family; NES covers NTSC. C64 supports composite and separate luma/chroma; Amiga uses RGB. Existing defaults and raw screenshots are preserved.</p>
<p>For a modern display, use <code>--video modern --scale 2</code> or <code>--video modern-monitor --scale 2</code>. These retain the decoded signal and apply nearest scaling without CRT effects; Amiga interlace uses bob, with <code>modern-weave</code> and <code>modern-monitor-weave</code> available for paired fields.</p>
<p>This is an experimental retained-raster receiver: no RF stage, recovered sync/burst PLL, measured phosphor persistence or hardware-agreement claim. C64 gain/phase and receiver bandwidths still need calibration. The RGB Amiga path does not model an A520 adapter.</p>
'''+''.join(sections)+f'''<section><h2>Numerical and performance checks</h2><p>GPU: {html.escape(metrics['adapter'])}. Spectrum output agrees with the independent f64 CPU oracle within one byte level at three field phases and for the live runtime’s master-clock handoff. All four actual native windows were exercised, including both C64 connections.</p><table><thead><tr><th>Case</th><th>Median completed decode</th></tr></thead><tbody>{rows}</tbody></table><p>{html.escape(metrics['timing_scope'])}. These are local measurements, not a platform-wide guarantee.</p><p><a href="results.json">Results and source/image hashes</a> · <a href="validation.log">Validation log</a></p></section></html>'''
(out/'index.html').write_text(page)
print(out/'index.html')
