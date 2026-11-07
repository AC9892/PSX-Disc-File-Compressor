"""Convert local PS1/PS2 cover collections into separate WebP artwork data.

Requires Pillow with WebP support. Source trees are read only; output is never
placed in the executable or a disc archive.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

from PIL import Image, ImageOps


def process_one(source: Path, destination: Path, thumbnail: Path) -> tuple[int, int, int, str]:
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    if not destination.exists() or not thumbnail.exists():
        with Image.open(source) as original:
            image = ImageOps.exif_transpose(original)
            image.load()
            if image.mode not in ("RGB", "RGBA"):
                image = image.convert("RGBA" if "A" in image.getbands() or "transparency" in image.info else "RGB")
            destination.parent.mkdir(parents=True, exist_ok=True)
            thumbnail.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists():
                temporary = destination.with_suffix(".webp.tmp")
                image.save(temporary, format="WEBP", quality=85, method=4)
                temporary.replace(destination)
            if not thumbnail.exists():
                preview = image.copy()
                preview.thumbnail((160, 160), Image.Resampling.LANCZOS)
                temporary = thumbnail.with_suffix(".webp.tmp")
                preview.save(temporary, format="WEBP", quality=85, method=4)
                temporary.replace(thumbnail)
    return source.stat().st_size, destination.stat().st_size, thumbnail.stat().st_size, source_hash


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ps1", type=Path, required=True, help="PS1 clone directory containing covers/")
    parser.add_argument("--ps2", type=Path, required=True, help="PS2 clone directory containing covers/")
    parser.add_argument("--output", type=Path, required=True, help="Separate artwork data directory")
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    jobs: list[tuple[str, str, Path, Path, Path]] = []
    unmatched_names: list[str] = []
    for platform, root in (("ps1", args.ps1), ("ps2", args.ps2)):
        for source_kind, target_kind in (("default", "default"),):
            folder = root / "covers" / source_kind
            if not folder.is_dir():
                raise SystemExit(f"Missing cover directory: {folder}")
            for source in folder.iterdir():
                if source.is_file() and source.suffix.lower() in {".png", ".jpg", ".jpeg", ".webp"}:
                    serial = source.stem.upper()
                    if not serial or any(c not in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in serial):
                        unmatched_names.append(f"{platform}/{target_kind}/{source.name}")
                        output_kind = f"unmatched/{target_kind}"
                        serial = source.stem
                    else:
                        output_kind = target_kind
                    destination = args.output / platform / output_kind / f"{serial}.webp"
                    thumb = args.output / "thumbnails" / platform / output_kind / f"{serial}.webp"
                    jobs.append((platform, target_kind, source, destination, thumb))
    original_bytes = sum(source.stat().st_size for _, _, source, _, _ in jobs)
    args.output.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(args.output).free < max(original_bytes, 1024**3):
        raise SystemExit("Not enough free space for safe artwork conversion")
    results = {platform: {"default": 0} for platform in ("ps1", "ps2")}
    seen: set[str] = set()
    duplicates = 0
    failures: list[str] = []
    full_bytes = thumb_bytes = 0
    with ThreadPoolExecutor(max_workers=max(1, min(args.workers, 8))) as pool:
        futures = {pool.submit(process_one, source, dest, thumb): (platform, kind, source) for platform, kind, source, dest, thumb in jobs}
        for number, future in enumerate(as_completed(futures), 1):
            platform, kind, source = futures[future]
            try:
                _, full, thumb, source_hash = future.result()
                results[platform][kind] += 1
                full_bytes += full
                thumb_bytes += thumb
                if source_hash in seen:
                    duplicates += 1
                else:
                    seen.add(source_hash)
            except Exception as error:
                failures.append(f"{platform}/{kind}/{source.name}: {error}")
            if number % 1000 == 0:
                print(f"Processed {number}/{len(jobs)} covers; failures={len(failures)}", flush=True)
    report = {
        "counts": results, "original_bytes": original_bytes, "webp_bytes": full_bytes,
        "thumbnail_bytes": thumb_bytes, "saved_percent": round((1 - full_bytes / original_bytes) * 100, 2) if original_bytes else 0,
        "duplicate_source_images": duplicates, "failures": failures,
        "unmatched_names": unmatched_names,
        "quality": 85, "sources": {"ps1": "https://github.com/xlenore/psx-covers", "ps2": "https://github.com/xlenore/ps2-covers"},
        "license_note": "The source repositories declare no artwork license. This local artwork pack must not be redistributed without permission.",
    }
    (args.output / "conversion-report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    (args.output / "ATTRIBUTION.md").write_text(
        "# Local artwork sources\n\n"
        "PS1: https://github.com/xlenore/psx-covers — repository credits psxdatacenter.com and duckstation.org.\n\n"
        "PS2: https://github.com/xlenore/ps2-covers — repository credits pcsx2.net, psxdatacenter.com, gvcover.top, imkira3, and waifu2x.\n\n"
        "PS3 cached cover provider: https://www.gametdb.com/PS3 (covers served from art.gametdb.com).\n\n"
        "The repositories do not declare a license for the collected cover art. Game artwork belongs to its rights holders. "
        "This local converted collection is not included in the application or source repository and should not be redistributed without permission.\n",
        encoding="utf-8",
    )
    print(json.dumps({k: v for k, v in report.items() if k != "failures"}, indent=2), flush=True)
    print(f"Failed conversions: {len(failures)}", flush=True)


if __name__ == "__main__":
    main()
