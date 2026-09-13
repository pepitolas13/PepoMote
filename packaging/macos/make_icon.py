"""Genera packaging/macos/icon-1024.png: el logo de PepoMote (aro y punto,
la misma geometría que desktop/src/icon.rs) sobre la baldosa redondeada de
los iconos de macOS. Se renderiza a 4096 y se reduce con LANCZOS (antialias).
Uso: python packaging/macos/make_icon.py  (necesita Pillow)."""

from pathlib import Path

from PIL import Image, ImageDraw

SIZE = 4096
OUT = Path(__file__).with_name("icon-1024.png")

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# Baldosa: el icono ocupa el 80 % del lienzo (rejilla de macOS), esquinas al 22,5 %
inset = round(SIZE * 0.10)
tile = SIZE - 2 * inset
draw.rounded_rectangle((inset, inset, SIZE - inset, SIZE - inset), radius=round(tile * 0.225), fill=(0xF4, 0xF6, 0xF7, 255))

# Logo (icon.rs::logo_rgba): aro |d − 0.42·s| < 0.10·s y punto d < 0.13·s, en azul
s = tile * 0.78
c = SIZE / 2
blue = (0x3F, 0xA9, 0xF5, 255)
r_outer, r_ring, r_dot = s * 0.42, s * 0.10, s * 0.13
draw.ellipse((c - r_outer - r_ring, c - r_outer - r_ring, c + r_outer + r_ring, c + r_outer + r_ring), fill=blue)
draw.ellipse((c - r_outer + r_ring, c - r_outer + r_ring, c + r_outer - r_ring, c + r_outer - r_ring), fill=(0xF4, 0xF6, 0xF7, 255))
draw.ellipse((c - r_dot, c - r_dot, c + r_dot, c + r_dot), fill=blue)

img.resize((1024, 1024), Image.LANCZOS).save(OUT, optimize=True)
print(OUT, OUT.stat().st_size, "bytes")
