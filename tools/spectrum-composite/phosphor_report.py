#!/usr/bin/env python3
"""Package unmodified production phosphor/CRT renderings for comparison."""
from pathlib import Path
import hashlib
import json
import re

root = Path(__file__).resolve().parents[2]
out = root / 'target/phosphor-validation'
log = (root / 'target/phosphor-validation.log').read_text()
assert 'PASS:' in log
sources = ['crates/emu198x-native-video/src/lib.rs', 'crates/emu198x-native-video/src/shader.wgsl',
           'crates/emu198x-native-video/src/phosphor.rs', 'crates/emu198x-native-video/src/phosphor.wgsl',
           'crates/emu198x-ui/src/lib.rs', 'crates/emu198x-ui/src/launch.rs',
           'tools/spectrum-composite/src/validate_phosphor.rs', 'tools/spectrum-composite/src/validate_interlace.rs',
           'tools/spectrum-composite/src/offscreen_crt.rs']
(out / 'results.json').write_text(json.dumps({
    'validation_log': log,
    'source_sha256': {p: hashlib.sha256((root/p).read_bytes()).hexdigest() for p in sources},
    'image_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.png')},
    'phosphor_median_ms': float(re.search(r'update: ([0-9.]+) ms', log).group(1)),
    'timing_scope': '30 warmed completed updates: upload, source-space phosphor pass and completion; excludes decoding, CRT, readback and window',
    'source': 'Synthetic 768×576 detail/motion plate, not a hardware capture',
    'model': 'Linear RGB whole-raster recharge and exponential decay, one provisional equal-channel 1/e time constant; updated per machine field/frame',
    'playback': 'Browser illustration at nominal 20 ms per PAL field, not calibrated host cadence',
}, indent=2)+'\n')
(out / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>CRT phosphor afterglow</title><style>body{font:17px/1.5 system-ui;max-width:1050px;margin:32px auto;padding:0 20px;background:#14171b;color:#eee}button,select{font:inherit;padding:7px 10px;margin:4px}img{display:block;width:100%;image-rendering:pixelated}a{color:#acd0ff}p{max-width:850px}</style>
<h1>CRT phosphor afterglow</h1><p>The production GPU phosphor pass and CRT shader render this synthetic retained raster. Both fields display together steadily; switching field identity does not change this static picture.</p>
<label>Phosphor <select id="tau"><option value="0">No persistence</option><option value="6" selected>6 ms decay · provisional default</option><option value="20">20 ms decay · longer afterglow</option></select></label>
<button id="step">Next field</button><button id="play">Play fields</button><span id="field"></span>
<img id="picture" alt="Synthetic interlace plate rendered through the production afterglow and CRT shaders">
<p>The decay value is the time for light to fall to 1/e (about 37%) of its previous level. Light from preceding pictures survives until new drive replaces it. Longer persistence reduces abrupt field changes and leaves stronger motion trails.</p>
<p>Use <code>--phosphor-ms 0</code>, <code>--phosphor-ms 6</code> or <code>--phosphor-ms 20</code> with <code>--video monitor</code>, <code>signal</code> or <code>crt</code>. Modern, raw and LCD modes bypass afterglow.</p>
<p>These values are illustrative, not measured profiles for named monitors. The model uses equal-channel exponential decay and ideal fast recharge, updated once per emulated field/frame. The field controls exercise alternating metadata; native presentation deliberately omits interlace flicker.</p>
<p><a href="results.json">GPU checks, local timing and image/source hashes</a> · <a href="../interlace-validation/index.html">Earlier offscreen field experiment</a></p>
<script>
const picture=document.querySelector('#picture'),tau=document.querySelector('#tau'),label=document.querySelector('#field');let parity=0,timer=null;
for(const t of [0,6,20])for(const p of ['even','odd']){const img=new Image();img.src='crt-'+t+'-'+p+'.png';}
function show(){picture.src='crt-'+tau.value+'-'+(parity?'odd':'even')+'.png';label.textContent=parity?'Short field · odd rows':'Long field · even rows';}
tau.onchange=show;document.querySelector('#step').onclick=()=>{parity^=1;show();};document.querySelector('#play').onclick=()=>{if(timer){clearInterval(timer);timer=null;}else{timer=setInterval(()=>{parity^=1;show();},20);}document.querySelector('#play').textContent=timer?'Pause fields':'Play fields';};show();
</script></html>''')
print(out/'index.html')
