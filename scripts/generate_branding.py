"""Regenerate the checked-in app icons. Requires Pillow; never used at runtime."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
out = root / "src-tauri/icons"
out.mkdir(parents=True, exist_ok=True)
im = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.rounded_rectangle((24, 24, 1000, 1000), radius=240, fill="#111417", outline="#333c32", width=8)
d.rounded_rectangle((175, 335, 849, 689), radius=177, fill="#c1ff83")
d.line([(287, 517), (381, 517), (435, 426), (505, 611), (574, 472), (618, 517), (740, 517)], fill="#15200d", width=36, joint="curve")
im.save(out / "source.png")
for size in (32, 128, 256):
    im.resize((size, size), Image.Resampling.LANCZOS).save(out / f"{size}x{size}.png")
im.save(out / "icon.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
im.save(out / "icon.icns")
print("Generated PNG, ICO and ICNS application icons")
