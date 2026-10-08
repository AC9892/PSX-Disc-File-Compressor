"""Export per-chunk size comparisons from three verified PX1 archives."""

import argparse
import csv
import json
import statistics
import struct
from collections import Counter
from pathlib import Path


BEST_CODECS = {1: "zstd", 2: "XZ", 3: "delta+zstd", 4: "delta+XZ"}
COMBINED_CODECS = {5: "Brotli->XZ", 6: "XZ->Brotli"}


def entries(path: Path, expected_minor: int) -> tuple[dict, list[dict]]:
    with path.open("rb") as archive:
        header = archive.read(96)
        if len(header) != 96 or header[:4] != b"PX1\0":
            raise ValueError(f"Invalid PX1 header: {path}")
        major, minor = struct.unpack_from("<HH", header, 4)
        if (major, minor) != (1, expected_minor):
            raise ValueError(f"Expected PX1 v1.{expected_minor}: {path}")
        manifest_offset, manifest_len = struct.unpack_from("<QQ", header, 16)
        table_offset, table_len = struct.unpack_from("<QQ", header, 48)
        count = struct.unpack_from("<Q", header, 72)[0]
        entry_size = 56 if minor == 0 else 60
        if table_len != count * entry_size:
            raise ValueError(f"Unexpected chunk table size: {path}")
        archive.seek(manifest_offset)
        manifest = json.loads(archive.read(manifest_len))
        archive.seek(table_offset)
        rows = []
        for index in range(count):
            entry = archive.read(entry_size)
            if len(entry) != entry_size:
                raise ValueError(f"Truncated chunk table: {path}")
            raw_offset, _, raw_size, compressed_size = struct.unpack_from("<QQII", entry)
            codec = struct.unpack_from("<I", entry, 56)[0] if minor else 1
            rows.append({
                "index": index + 1,
                "offset": raw_offset,
                "raw_size": raw_size,
                "compressed_size": compressed_size,
                "source_sha256": entry[24:56],
                "codec": codec,
            })
        return manifest, rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("zstd19", type=Path, help="PX1 v1.0 level-19 zstd archive")
    parser.add_argument("best_size", type=Path, help="PX1 v1.1 Best Size archive")
    parser.add_argument("combined", type=Path, help="PX1 v1.2 Brotli+XZ archive")
    parser.add_argument("output", type=Path, help="Destination CSV")
    args = parser.parse_args()

    sources = [entries(args.zstd19, 0), entries(args.best_size, 1), entries(args.combined, 2)]
    if any(len(rows) != len(sources[0][1]) for _, rows in sources):
        raise ValueError("Archive chunk counts differ")
    source_hashes = {(manifest["bin"]["sha256"], manifest["cue"]["sha256"]) for manifest, _ in sources}
    if len(source_hashes) != 1:
        raise ValueError("Archives preserve different source files")

    totals = Counter()
    orders = Counter()
    best_wins = Counter()
    savings_vs_best = []
    savings_vs_zstd = []
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", newline="", encoding="utf-8") as result:
        writer = csv.writer(result)
        writer.writerow([
            "chunk", "bin_offset_bytes", "original_bytes", "zstd19_bytes",
            "best_size_bytes", "best_size_codec", "combined_bytes", "combined_order",
            "combined_saved_vs_zstd_bytes", "combined_saved_vs_best_bytes",
        ])
        for zstd, best, combined in zip(*(rows for _, rows in sources)):
            identity = (zstd["offset"], zstd["raw_size"], zstd["source_sha256"])
            for other in (best, combined):
                if (other["offset"], other["raw_size"], other["source_sha256"]) != identity:
                    raise ValueError(f"Chunk {zstd['index']} preserves different source bytes")
            if best["codec"] not in BEST_CODECS or combined["codec"] not in COMBINED_CODECS:
                raise ValueError(f"Unexpected codec at chunk {zstd['index']}")
            zstd_size, best_size, combined_size = (row["compressed_size"] for row in (zstd, best, combined))
            writer.writerow([
                zstd["index"], zstd["offset"], zstd["raw_size"], zstd_size,
                best_size, BEST_CODECS[best["codec"]], combined_size,
                COMBINED_CODECS[combined["codec"]], zstd_size - combined_size,
                best_size - combined_size,
            ])
            totals.update({"original": zstd["raw_size"], "zstd19": zstd_size,
                           "best_size": best_size, "combined": combined_size})
            orders[COMBINED_CODECS[combined["codec"]]] += 1
            best_wins["combined_smaller"] += combined_size < best_size
            best_wins["combined_equal"] += combined_size == best_size
            best_wins["combined_larger"] += combined_size > best_size
            savings_vs_best.append((best_size - combined_size, zstd["index"]))
            savings_vs_zstd.append((zstd_size - combined_size, zstd["index"]))

    print(json.dumps({"chunks": len(sources[0][1]), "payload_totals": totals,
                      "combined_orders": orders, "versus_best_size": best_wins,
                      "vs_best_per_chunk": {
                          "median_saved_bytes": statistics.median(value for value, _ in savings_vs_best),
                          "total_positive_savings_bytes": sum(value for value, _ in savings_vs_best if value > 0),
                          "total_regression_bytes": -sum(value for value, _ in savings_vs_best if value < 0),
                          "largest_saving": max(savings_vs_best),
                          "largest_regression": min(savings_vs_best),
                      },
                      "vs_zstd_winning_chunks": sum(value > 0 for value, _ in savings_vs_zstd),
                      "csv": str(args.output)}, indent=2))


if __name__ == "__main__":
    main()
