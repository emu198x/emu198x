#!/usr/bin/env python3
"""Make a synchronized movie from production-rendered Amiga picture replays."""
from pathlib import Path
import subprocess
from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parents[2]
out = root / 'target/amiga-display-preview'
font = ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf', 25)
small = ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial.ttf', 19)
columns = [('CRT: no afterglow', 'crt-0'), ('CRT: current 6 ms afterglow', 'crt-6'), ('Modern monitor: weave', 'modern-weave')]
for parity in ['even', 'odd']:
    board = Image.new('RGB', (1920, 850), '#181b20')
    draw = ImageDraw.Draw(board)
    draw.text((20, 14), 'AmigaDOS picture — alternating field replay', font=font, fill='white')
    for i, (label, name) in enumerate(columns):
        x = i * 640
        picture = Image.open(out / (name + (f'-{parity}' if name.startswith('crt') else '') + '.png')).convert('RGB')
        draw.text((x + 16, 65), label, font=font, fill='white')
        board.paste(picture.resize((640, 480), Image.Resampling.LANCZOS), (x, 108))
        draw.text((x + 16, 614), 'Text detail · original rendered pixels', font=small, fill='#bac5d3')
        board.paste(picture.crop((160, 76, 800, 256)), (x, 650))
    board.save(out / f'comparison-{parity}.png')
# Two rendered fields alternate at nominal PAL cadence. Slow playback is
# separately labelled; neither recording claims calibrated monitor timing.
for name, fps in [('comparison', 50), ('comparison-slow', 5)]:
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-framerate', str(fps),
                    '-pattern_type', 'glob', '-i', str(out / 'comparison-*.png'),
                    '-vf', 'loop=loop=-1:size=2:start=0', '-t', '10', '-c:v', 'libx264',
                    '-crf', '16', '-pix_fmt', 'yuv420p', '-movflags', '+faststart',
                    str(out / f'{name}.mp4')], check=True)
(out / 'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Amiga display in motion</title><style>body{margin:24px;background:#181b20;color:#eee;font:18px/1.5 system-ui}main{max-width:1920px;margin:auto}video{width:100%;display:block}button{font:inherit;padding:8px 14px;margin:0 8px 16px 0}p{max-width:1000px}a{color:#acd0ff}</style><main>
<h1>Amiga display in motion</h1><p>All three pictures play together. Watch the white text and window edges; the modern monitor holds both fields steady.</p>
<button id="normal">Normal speed</button><button id="slow">Slow motion ×10</button><span id="speed">50 fields per second</span>
<video id="movie" src="comparison.mp4" autoplay muted loop playsinline controls></video>
<p>This is a real AmigaDOS 1.3 picture replayed through the production renderer as alternating fields. The captured program used a progressive display. This demonstrates the display model, not an interlaced application recording.</p>
<p>Browser playback and your monitor refresh can change how flicker appears. The 6 ms afterglow is provisional, not a measured monitor profile. Slow motion exposes field changes but does not reproduce their perceived intensity.</p>
<p><a href="comparison.mp4">Open the movie</a> · <a href="comparison-slow.mp4">Open slow motion</a></p></main>
<script>const movie=document.querySelector('#movie'),speed=document.querySelector('#speed');function show(slow){movie.src=slow?'comparison-slow.mp4':'comparison.mp4';speed.textContent=slow?'Slow motion ×10':'50 fields per second';movie.play();}document.querySelector('#normal').onclick=()=>show(false);document.querySelector('#slow').onclick=()=>show(true);</script></html>''')
print(out / 'index.html')
