#!/usr/bin/env python3
"""Rebuild the decorative README GIF. Dev-only: pip install cairosvg pillow.
No network requests, live data, browser scripts or runtime dependencies.
"""
import io
import math
from pathlib import Path
import cairosvg
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'assets/abyss-dragon.svg').read_text()
frames = []
for i in range(24):
    phase = i / 24 * 2 * math.pi
    svg = source.replace('class="ringA"', f'transform="rotate({i*15} 400 310)"')
    svg = svg.replace('class="ringB"', f'transform="rotate({-i*15} 400 310)"')
    svg = svg.replace('class="dragonbody"', f'transform="translate(0 {-3*math.sin(phase)})"')
    svg = svg.replace('class="wingL"', f'transform="rotate({-2*math.sin(phase)} 380 280)"')
    svg = svg.replace('class="wingR"', f'transform="rotate({2*math.sin(phase)} 420 280)"')
    png = cairosvg.svg2png(bytestring=svg.encode(), output_width=640, output_height=496)
    frames.append(Image.open(io.BytesIO(png)).convert('RGB'))
frames[0].save(ROOT / 'assets/abyss-dragon.gif', save_all=True,
               append_images=frames[1:], duration=100, loop=0, optimize=True)
print('Rendered 24-frame 640x496 decorative GIF (2.4 second loop).')
