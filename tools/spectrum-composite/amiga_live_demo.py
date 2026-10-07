#!/usr/bin/env python3
"""Build project-owned firmware for a live 640×512 interlaced RGB demo.

Register layout: A500/A2000 Technical Reference Manual (1987), BPLxPT,
BPLxMOD, BPLCON0 and VPOSR descriptions in the shared reference library.
The picture is derived from our progressive AmigaDOS capture; this firmware,
not Workbench, selects LACE and supplies alternating bitmap rows to DMA.
"""
from pathlib import Path
import struct
from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parents[2]
out = root / 'target/amiga-display-preview'
# Include both window edges before fitting the capture into the guest's
# 640-pixel display. A 640-wide crop at x=64 cuts off the right edge.
image = (Image.open(out / 'workbench.png').convert('RGB')
         .crop((64, 32, 736, 544))
         .resize((640, 512), Image.Resampling.NEAREST))
draw = ImageDraw.Draw(image)
font = ImageFont.truetype('/System/Library/Fonts/Supplemental/Courier New.ttf', 15)
draw.rectangle((12, 250, 626, 415), fill=(0, 85, 170), outline='white', width=1)
for y, text in [(264, 'LIVE INTERLACED RGB MONITOR DEMO'),
                (296, '640 x 512 pixels. Two alternating fields.'),
                (328, 'Watch the thin edges and small lettering.'),
                (360, 'Compare Monitor with Modern Monitor.'),
                (392, 'Guest demo: not an interlaced Workbench session.')]:
    draw.text((24, y), text, font=font, fill='white')
# Four OCS RGB colours, packed into two separate full-height bitplanes.
colours = [(0, 85, 170), (255, 255, 255), (0, 0, 0), (255, 136, 0)]
indices = [min(range(4), key=lambda i: sum((a-b)**2 for a,b in zip(pixel,colours[i])))
           for pixel in image.get_flattened_data()]
planes = bytearray()
for plane in range(2):
    for y in range(512):
        for x in range(0, 640, 8):
            planes.append(sum(((indices[y*640+x+b] >> plane) & 1) << (7-b) for b in range(8)))

code = bytearray(struct.pack('>II', 0x80000, 0xF80008))
labels, fixes = {}, []
def words(*values):
    code.extend(struct.pack('>'+'H'*len(values), *values))
def long(value):
    code.extend(struct.pack('>I', value))
def reg(offset, value):
    words(0x33FC, value); long(0xDFF000 + offset)
def label(name):
    labels[name] = len(code)
def branch(op, name):
    words(op); fixes.append((len(code), name)); words(0)
# Disable interrupts and DMA while copying the bitmap to chip RAM.
words(0x46FC, 0x2700)
reg(0x096, 0x7FFF); reg(0x09A, 0x7FFF)
words(0x207C); long(0xF81000)  # A0 source in firmware
words(0x227C); long(0x10000)   # A1 destination in chip RAM
words(0x303C, len(planes)//2 - 1)
label('copy'); words(0x32D8); branch(0x51C8, 'copy')  # MOVE.W; DBRA
for offset, value in [(0x08E,0x2C81),(0x090,0x2CC1),(0x092,0x003C),
                      (0x094,0x00D4),(0x108,80),(0x10A,80),
                      (0x100,0xA204),(0x102,0),(0x104,0),
                      (0x180,0x05A),(0x182,0xFFF),(0x184,0),(0x186,0xF80)]:
    reg(offset,value)
words(0x323C, 0xFFFF)  # D1 previous LOF, force first pointer setup
label('field')
words(0x3039); long(0xDFF004)
words(0x0240,0x8000,0xB240)  # mask LOF; CMP.W D0,D1
branch(0x6700,'field')
words(0x3200)  # remember LOF
words(0x243C); long(0x10000)
words(0x263C); long(0x1A000)
words(0x0800,15); branch(0x6600,'pointers')
words(0x0682); long(80)
words(0x0683); long(80)
label('pointers')
words(0x23C2); long(0xDFF0E0)
words(0x23C3); long(0xDFF0E4)
reg(0x096,0x8300)
branch(0x6000,'field')
for offset, name in fixes:
    code[offset:offset+2] = struct.pack('>h',labels[name]-offset)
assert len(code) < 4096
rom = bytearray(256*1024)
rom[:len(code)] = code
rom[4096:4096+len(planes)] = planes
(out / 'live-interlace-demo.rom').write_bytes(rom)
image.save(out / 'live-demo-bitmap.png')
print(out / 'live-interlace-demo.rom')
