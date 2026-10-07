#!/usr/bin/env python3
"""Reproduce the offline experiment and write an inspectable HTML comparison.

Requires Python 3 + Pillow, Cargo and a native wgpu adapter. No ROMs needed.
Run from anywhere; all output goes to an explicitly chosen directory.
"""
from pathlib import Path
import argparse
import hashlib
import html
import json
import platform
import subprocess
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]
W, H = 352, 296


def run(*args):
    subprocess.run([str(a) for a in args], check=True)


def fixtures(extra_png):
    bars = Image.new("L", (W, H), 7)
    draw = ImageDraw.Draw(bars)
    for bright in range(2):
        for colour in range(8):
            draw.rectangle((48 + colour*32, 48 + bright*96, 79 + colour*32, 143 + bright*96), fill=colour+bright*8)
    yield "colour-bars", bars.tobytes(), "Normal above, bright below. Tests the ULA voltage table and receiver colour response."
    mono = Image.new("L", (W, H), 7)
    draw = ImageDraw.Draw(mono)
    for x in range(48,304):
        draw.line((x,48,x,239), fill=15 if x%2 else 0)
    yield "mono-stripes", mono.tobytes(), "One-pixel black/white stripes. The source has zero chroma; any decoded colour comes from signal separation."
    patterns = Image.new("L", (W,H), 7)
    draw = ImageDraw.Draw(patterns)
    for band, pair in enumerate([(0,15),(2,4),(1,6),(3,5)]):
        for block, period in enumerate([1,2,4,8]):
            for y in range(48+band*48,96+band*48):
                for x in range(48+block*64,112+block*64):
                    patterns.putpixel((x,y),pair[((x//period)+(y//period))%2])
    yield "dither-grid", patterns.tobytes(), "Four colour pairs, each at 1/2/4/8-pixel checker sizes. Tests loss of fine colour detail and dither blending."
    sources = [("spectrum-boot", REPO / "crates/runtime-sinclair-zx-spectrum/tests/goldens/spectrum-48k-boot.png",
        "Runtime boot golden, with its legacy normal RGB=205 recovered as indices. Raw comparison uses current RGB=194.")]
    if extra_png:
        sources.append(("runtime-capture",extra_png,"Supplied runtime capture: " + extra_png.name))
    for name,source,description in sources:
        im = Image.open(source).convert("RGB")
        if im.size != (W,H):
            raise ValueError(f"unexpected input dimensions: {im.size}")
        pixels = bytearray()
        for r,g,b in im.get_flattened_data():
            active = [v for v in (r,g,b) if v]
            if any(v not in (194,205,255) for v in active) or len(set(active)) > 1:
                raise ValueError("input contains a non-Spectrum palette colour")
            pixels.append((2 if r else 0) | (4 if g else 0) | (1 if b else 0) | (8 if active and active[0] == 255 else 0))
        yield name,bytes(pixels),description


def images(directory):
    for mode in ("raw","analogue","separated","composite"):
        im = Image.frombytes("RGBA", (W,H), (directory / f"{mode}.rgba").read_bytes())
        im.save(directory / f"{mode}.png")
        im.resize((W*3,H*3),Image.Resampling.NEAREST).save(directory / f"{mode}-3x.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=REPO / "target/spectrum-composite-comparison")
    parser.add_argument("--png", type=Path, help="also compare a raw 352x296 Spectrum capture")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True,exist_ok=True)
    run("cargo","build","--release","--locked","--manifest-path", ROOT/"Cargo.toml")
    binary = ROOT / "target/release/composite-experiment"
    gpu = ROOT / "target/release/render-crt"
    decoder = ROOT / "target/release/decode-gpu"
    records = []
    for name, indices, description in fixtures(args.png):
        directory = out/name
        directory.mkdir(exist_ok=True)
        path = directory / "source.idx"
        path.write_bytes(indices)
        run(binary,path,directory)
        run(decoder,path,directory)
        images(directory)
        Image.frombytes("RGBA",(W,H),(directory/"gpu-composite.rgba").read_bytes()).save(directory/"gpu-composite.png")
        Image.frombytes("RGBA",(W*3,H*3),(directory/"gpu-composite-crt.rgba").read_bytes()).save(directory/"gpu-composite-crt.png")
        for source, target in (("raw","current-crt"),("composite","composite-crt")):
            run(gpu,directory/f"{source}.rgba",directory/f"{target}.rgba")
            Image.frombytes("RGBA",(W*3,H*3),(directory/f"{target}.rgba").read_bytes()).save(directory/f"{target}.png")
        comparison = Image.new("RGB",(W*6,H*3+32),"white")
        comparison.paste(Image.open(directory/"current-crt.png"),(0,32))
        comparison.paste(Image.open(directory/"gpu-composite-crt.png"),(W*3,32))
        labels = ImageDraw.Draw(comparison)
        labels.text((12,8),"Current CRT",fill="black")
        labels.text((W*3+12,8),"GPU composite + same CRT",fill="black")
        comparison.save(directory/"comparison.png")
        metrics = json.loads((directory/"metrics.json").read_text())
        metrics["gpu"] = json.loads((directory/"gpu-metrics.json").read_text())
        metrics.update(name=name,description=description,input_sha256=hashlib.sha256(indices).hexdigest())
        records.append(metrics)
    # Sensitivity checks: sample-rate convergence and phase evolution across
    # successive 312-line fields. No visual fit to a reference screenshot.
    sensitivity = []
    base = out/"mono-stripes"
    for label,spp,phase,field in (("8x",8,0,0),("phase-quarter",4,0.25,0),("next-field",4,0,1)):
        directory = out/f"sensitivity-{label}"
        run(binary,base/"source.idx",directory,spp,phase,field)
        run(decoder,base/"source.idx",directory,spp,phase,field)
        images(directory)
        reference = (base/"composite.rgba").read_bytes()
        result = (directory/"composite.rgba").read_bytes()
        diffs = [a-b for i,(a,b) in enumerate(zip(reference,result)) if i%4 != 3]
        metrics = json.loads((directory/"metrics.json").read_text())
        metrics["gpu"] = json.loads((directory/"gpu-metrics.json").read_text())
        metrics.update(name=label,display_rgb_rmse_8bit=(sum(d*d for d in diffs)/len(diffs))**0.5)
        sensitivity.append(metrics)
    evidence = {
        "machine":platform.platform(),
        "repo_commit":subprocess.check_output(["git","-C",str(REPO),"rev-parse","HEAD"],text=True).strip(),
        "production_shader_sha256":hashlib.sha256((REPO/"crates/emu198x-native-video/src/shader.wgsl").read_bytes()).hexdigest(),
        "experiment_source_sha256":{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()
            for p in [ROOT/"src/lib.rs",ROOT/"src/main.rs",ROOT/"src/render_crt.rs",ROOT/"src/decode_gpu.rs",ROOT/"src/decode.wgsl",ROOT/"src/offscreen_crt.rs",ROOT/"report.py",ROOT/"Cargo.lock"]},
        "fixtures":records,"sensitivity":sensitivity,
    }
    if args.png:
        evidence["extra_capture"] = {"path":str(args.png.resolve()),"sha256":hashlib.sha256(args.png.read_bytes()).hexdigest()}
    (out/"results.json").write_text(json.dumps(evidence,indent=2)+"\n")
    sections = []
    for r in records:
        name = r["name"]
        sections.append(f'''<section><h2>{html.escape(name)}</h2><p>{html.escape(r["description"])}</p>
        <div class="compare"><img src="{name}/current-crt.png" alt="Current production CRT">
        <div class="overlay"><img src="{name}/gpu-composite-crt.png" alt="GPU composite through the same CRT shader"></div></div>
        <label>Current CRT ← <input type="range" min="0" max="100" value="50" oninput="this.parentElement.previousElementSibling.querySelector('.overlay').style.width=this.value+'%'"> → GPU composite + same CRT</label>
        <div class="grid">{''.join(f'<figure><img src="{name}/{mode}-3x.png" alt="{mode}"><figcaption>{caption}</figcaption></figure>' for mode,caption in (("raw","Current raw palette"),("analogue","ULA analogue levels → ideal RGB"),("separated","Separated Y/C bandwidth control"),("composite","Composite receiver, before CRT")))}</div>
        <p>CPU decoder: {r["median_decode_ms"]:.2f} ms/frame. GPU decode + CRT: {r["gpu"]["median_decode_crt_ms"]:.2f} ms median / {r["gpu"]["p95_decode_crt_ms"]:.2f} ms p95. GPU versus CPU: {r["gpu"]["display_rgb_rmse_8bit"]:.4f}/255 RGB RMSE, maximum channel difference {r["gpu"]["max_rgb_byte_delta"]}.</p><p><a href="{name}/gpu-composite-crt.png">GPU-rendered comparison image</a>. Composite versus separated RGB RMSE: {r["composite_vs_separated_rgb_rmse"]:.4f} (unclipped amplitudes).</p></section>''')
    rows = ''.join(f'<tr><td>{r["name"]}</td><td>{r["median_decode_ms"]:.2f} ms</td><td>{r["display_rgb_rmse_8bit"]:.2f} / 255</td></tr>' for r in sensitivity)
    page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Spectrum composite experiment</title>
    <style>body{font:16px/1.5 system-ui,sans-serif;color:#222;background:#fafafa;margin:40px auto;max-width:1120px;padding:0 24px}h1,h2{line-height:1.2}section{border-top:1px solid #ccc;margin-top:48px;padding-top:20px}img{max-width:100%;display:block}figure{margin:0}figcaption{margin:8px 0 20px}.grid{display:grid;grid-template-columns:1fr 1fr;gap:20px}.compare{position:relative;width:100%;max-width:1056px}.compare>img{width:100%}.overlay{position:absolute;inset:0 auto 0 0;width:50%;overflow:hidden;border-right:2px solid #e66}.overlay img{width:1056px;max-width:none}label{display:block;margin:16px 0 28px}input{width:35%;vertical-align:middle}table{border-collapse:collapse}td,th{padding:8px 20px;text-align:left;border-bottom:1px solid #ddd}a{color:#2459a3}@media(max-width:700px){.grid{grid-template-columns:1fr}} </style>
    <h1>Spectrum composite experiment</h1><p>A reproducible offline experiment in the missing analogue video stage. The slider compares the <strong>unmodified production CRT shader</strong> against composite decoding fed through that same shader. All comparisons use fixed 3× square-pixel geometry; they do not validate television placement or aspect ratio.</p>
    <p><strong>Evidence boundary:</strong> these are generated pictures, not hardware matches. ULA levels come from Smith Chapter 16. Chroma gain, ideal carrier reference, windowed-sinc receiver bandwidths (3 MHz Y / 1.3 MHz chroma), and one-line chroma averaging are explicit assumptions. Sync and burst are synthesised but do not control decoder lock. No RF, vertical sync, phosphor persistence or live machine output is modelled.</p>
    <p>Normal and bright chroma are identical at the encoder. Bright-yellow luminance uses the provisional shadowed table value; it requires independent confirmation. The framebuffer adapter only covers static 48K frames.</p>'''
    cpu_min = min(r["median_decode_ms"] for r in records)
    cpu_max = max(r["median_decode_ms"] for r in records)
    convergence = sensitivity[0]["display_rgb_rmse_8bit"]
    phase_change = sensitivity[1]["display_rgb_rmse_8bit"]
    page += f'''<p><strong>Observed:</strong> fine monochrome detail generates colour in the combined path, while the separated control stays achromatic. Doubling the sampling rate changes the difficult stripe output by {convergence:.2f}/255 RGB RMSE; changing carrier phase by a quarter-cycle changes it by {phase_change:.2f}/255. CPU decoding takes {cpu_min:.1f}–{cpu_max:.1f} ms/frame here, exceeding the approximately 20 ms budget for 50 Hz. This supports further investigation of the mechanism; hardware amplitudes and receiver response still need validation.</p>'''
    gpu_rows = ''.join(f'<tr><td>{r["name"]}</td><td>{r["median_decode_ms"]:.2f}</td><td>{r["gpu"]["median_frame_ms"]:.2f}</td><td>{r["gpu"]["median_decode_crt_ms"]:.2f}</td><td>{r["gpu"]["p95_decode_crt_ms"]:.2f}</td><td>{r["gpu"]["display_rgb_rmse_8bit"]:.4f}</td></tr>' for r in records+sensitivity)
    page += f'''<section><h2>GPU acceleration with CPU verification</h2><p>Adapter: {html.escape(records[0]["gpu"]["adapter"])}. The GPU preserves the CPU signal model and shared tables. Each run checks unclipped YUV (maximum accepted difference 0.0001) and displayed RGB (maximum accepted difference 1/255); a mismatch aborts the report.</p><table><tr><th>Input</th><th>CPU decode ms</th><th>GPU decode ms</th><th>GPU + CRT ms</th><th>GPU + CRT p95 ms</th><th>RGB RMSE /255</th></tr>{gpu_rows}</table><p>Each GPU timing includes per-frame index and phase-reference uploads, encoding, filtering, RGB resolve, submission and waiting for completion. The CRT column also renders the production shader at 1056×888 directly from the decoded GPU texture, without CPU readback between stages. Eight changing fields warm the path; sixty completed frames supply each distribution. Pipeline creation, fixed tables, verification readback and file exports are excluded. These are offscreen timings, excluding machine execution, window composition and vsync.</p></section>'''
    page += ''.join(sections)
    page += f'''<section><h2>Sampling and phase sensitivity</h2><p>Compared with the 4× default monochrome-stripe output. Phase changes affect fine detail because the carrier is independent of the pixel clock. The 8× comparison also tests output sample alignment and sampling error.</p><table><tr><th>Experiment</th><th>CPU decoder</th><th>Displayed RGB RMSE</th></tr>{rows}</table><div class="grid">'''
    for r in sensitivity:
        label=r["name"]
        page+=f'<figure><img src="sensitivity-{label}/composite-3x.png" alt="{label}"><figcaption>{label}</figcaption></figure>'
    page += '</div><p><a href="results.json">Full measurements and provenance</a>. Timings are medians of three warm release runs, excluding filter/oscillator setup, exports and GPU rendering; allocations are included. CPU prototype costs are not GPU estimates.</p></section><script>function resize(){document.querySelectorAll(".compare").forEach(c=>c.querySelector(".overlay img").style.width=c.clientWidth+"px")}window.addEventListener("resize",resize);resize()</script></html>'
    (out/"index.html").write_text(page)
    print(out/"index.html")


if __name__ == "__main__":
    main()
