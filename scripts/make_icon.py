"""Build the Windows app icon from the checked-in artwork.

Run with a Python environment that has Pillow installed:
    python scripts/make_icon.py
"""

from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src-tauri" / "icons" / "source.png"
TARGET = ROOT / "src-tauri" / "icons" / "icon.ico"
SIZES = (16, 24, 32, 48, 64, 128, 256)


def main() -> None:
    source = Image.open(SOURCE).convert("RGBA")
    bounds = source.getbbox()
    if bounds is None:
        raise ValueError(f"Icon source is fully transparent: {SOURCE}")

    artwork = source.crop(bounds)
    canvas = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
    artwork.thumbnail((256, 256), Image.Resampling.LANCZOS)
    canvas.alpha_composite(
        artwork, ((256 - artwork.width) // 2, (256 - artwork.height) // 2)
    )
    canvas.save(TARGET, format="ICO", sizes=[(size, size) for size in SIZES])
    print(f"Created {TARGET} from {SOURCE}")


if __name__ == "__main__":
    main()
