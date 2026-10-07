#!/usr/bin/env python3
"""Package unmodified production-shader field images for manual comparison."""
from pathlib import Path
import hashlib
import json

root = Path(__file__).resolve().parents[2]
out = root / 'target/interlace-validation'
log = (root / 'target/interlace-validation.log').read_text()
assert 'PASS:' in log
sources = ['crates/emu198x-native-video/src/lib.rs', 'crates/emu198x-native-video/src/shader.wgsl',
           'crates/runtime-commodore-amiga/src/runtime.rs', 'crates/emu198x-shell/src/host.rs',
           'tools/spectrum-composite/src/validate_interlace.rs', 'tools/spectrum-composite/src/offscreen_crt.rs']
(out / 'results.json').write_text(json.dumps({
    'validation_log': log,
    'source_sha256': {p: hashlib.sha256((root/p).read_bytes()).hexdigest() for p in sources},
    'image_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.png')},
    'diagnostic_rom_sha256': hashlib.sha256((out/'synthetic-lace.rom').read_bytes()).hexdigest(),
    'source': 'Synthetic 768×576 fine-detail/motion plate; not a hardware capture',
    'playback': 'Browser illustration at nominal 20 ms per PAL field; not a measured host cadence',
}, indent=2) + '\n')
(out / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Amiga field presentation</title><style>body{font:17px/1.5 system-ui;max-width:1050px;margin:32px auto;padding:0 20px;background:#14171b;color:#eee}button,select{font:inherit;padding:7px 10px;margin:4px}img{display:block;width:100%;image-rendering:pixelated}a{color:#acd0ff}p{max-width:850px}</style>
<h1>Amiga field presentation</h1><p>The production shader renders this synthetic fine-detail/motion plate. The top alternates bright and dark rows; the lower bar moves between fields. It is a diagnostic image, not an Amiga program or hardware capture.</p>
<label>Display <select id="mode"><option value="crt">CRT monitor</option><option value="bob">Modern display: bob</option><option value="weave">Modern display: weave</option></select></label>
<button id="step">Next field</button><button id="play">Play fields</button><span id="field"></span>
<img id="picture" alt="Synthetic detail and motion plate rendered through the production field shader">
<p id="description"></p><p>CRT presents one field with alternating half-line beam position. Bob duplicates the current field’s rows. Weave combines fields, preserving fine static detail and combing the moving bar. Live weave falls back to bob until a consecutive opposite-parity pair exists.</p>
<p>A project-owned <a href="synthetic-lace.rom">diagnostic ROM</a> also enables LACE, polls LOF and writes field-dependent colours through the real emulated CPU. <a href="guest-lace-raw.png">Its retained raw raster</a> contains both fields. The runtime and all three live display choices were exercised.</p>
<p>Playback illustrates nominal PAL field alternation at 20 ms per field; browser timing is not a calibrated display. These images isolate field presentation with persistence disabled. <a href="../phosphor-validation/index.html">Compare phosphor afterglow</a>. Motion-adaptive deinterlacing remains future work.</p><p><a href="results.json">Validation results and image/source hashes</a></p>
<script>
const picture=document.querySelector('#picture'),mode=document.querySelector('#mode'),label=document.querySelector('#field');
const descriptions={crt:'Fine rows flicker because they belong to different fields. The beam position shifts between fields.',bob:'Only the current field is shown. Thin detail flickers and vertical resolution is reduced.',weave:'Both fields appear together. Fine detail is retained, while the moving bar shows combing.'};
let parity=0,timer=null;
for(const name of ['crt-even','crt-odd','bob-even','bob-odd','weave']){const img=new Image();img.src=name+'.png';}
function show(){picture.src=mode.value==='weave'?'weave.png':mode.value+'-'+(parity?'odd':'even')+'.png';label.textContent=parity?'Short field · odd rows':'Long field · even rows';document.querySelector('#description').textContent=descriptions[mode.value];}
mode.onchange=show;document.querySelector('#step').onclick=()=>{parity^=1;show();};
document.querySelector('#play').onclick=()=>{if(timer){clearInterval(timer);timer=null;}else{timer=setInterval(()=>{parity^=1;show();},20);}document.querySelector('#play').textContent=timer?'Pause fields':'Play fields';};show();
</script></html>''')
print(out / 'index.html')
