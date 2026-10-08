"""Generate the small native Windows icon without external dependencies."""
from pathlib import Path
import struct

SIZE = 64
pixels = bytearray()
for y in range(SIZE - 1, -1, -1):
    for x in range(SIZE):
        dx, dy = x - 31.5, y - 31.5
        d2 = dx * dx + dy * dy
        if d2 < 8 * 8:
            color = (34, 36, 39, 255)
        elif d2 < 28 * 28:
            color = (184, 191, 197, 255)
            if 22 * 22 < d2 < 24 * 24:
                color = (122, 136, 148, 255)
        else:
            color = (34, 36, 39, 255)
        pixels.extend(color)

bitmap = struct.pack('<IIIHHIIIIII', 40, SIZE, SIZE * 2, 1, 32, 0,
                     len(pixels), 0, 0, 0, 0)
mask = bytes(((SIZE + 31) // 32) * 4 * SIZE)
image = bitmap + pixels + mask
header = struct.pack('<HHH', 0, 1, 1)
entry = struct.pack('<BBBBHHII', SIZE, SIZE, 0, 0, 1, 32, len(image), 22)
target = Path(__file__).resolve().parents[1] / 'src-tauri' / 'icons' / 'icon.ico'
target.parent.mkdir(exist_ok=True)
target.write_bytes(header + entry + image)
