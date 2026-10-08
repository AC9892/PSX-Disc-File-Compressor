# Codec selection for the two-layer PX1 mode

On 2026-10-08, 12 chunks spread across the real test BIN were compared. The chunks were at indices 0, 15, 30, 45, 60, 75, 90, 105, 120, 135, 150, and 174 (zero-based, 4 MiB except the final chunk). Their total original size was 48,600,128 bytes. The source BIN SHA-256 was `458e63272b480385a98dfd9c1c26cc6581d7841ac612a5ec3a600d5d7c40da26`.

| Lossless codec | Settings | Total sampled bytes |
| --- | --- | ---: |
| Brotli | Quality 11, 22-bit window | 32,610,422 |
| XZ / LZMA2 | Preset 6 | 33,139,064 |
| XZ / LZMA2 | Preset 9 | 33,139,104 |
| zstd | Level 19 | 33,997,575 |
| bzip2 | Level 9 | 35,940,948 |
| DEFLATE | Level 9 | 36,314,275 |

Brotli and XZ were the two smallest individual codecs in this sample. PX1 v1.2 therefore applies **both** to each chunk, tests both orders, and records the smaller two-layer result. These measurements are specific to this disc and sample; stacking two codecs is not guaranteed to beat the best single codec.

## Full-disc result

The full test disc contained 732,271,680 BIN bytes and 81 CUE bytes. The v1.2 combined archive was 515,448,806 bytes (491.57 MiB), or 70.39% of the uncompressed BIN/CUE size. That is 6,324,109 bytes (6.03 MiB) smaller than the earlier v1.1 Best Size archive and 20,887,430 bytes (19.92 MiB) smaller than the level-19 zstd archive. Brotli→XZ won on 145 chunks; XZ→Brotli won on 30.

The archive passed PX1's complete verification and was extracted to a separate folder. The restored BIN SHA-256 was `458e63272b480385a98dfd9c1c26cc6581d7841ac612a5ec3a600d5d7c40da26`; the restored CUE SHA-256 was `3470744c0dd65cea7f78025a5b21695433db8d38740827d8d74bf870487c7199`. Both match their original source files exactly. DuckStation playback is left for the user to test.

## Per-chunk data

The [175-row CSV](COMPRESSION_RESULTS.csv) compares the same original 4 MiB BIN chunks across the level-19 zstd, v1.1 Best Size, and v1.2 combined archives. It records original and compressed byte counts, the winning codec order, and byte savings for each chunk. The exporter checks that every archive has the same stored source BIN and CUE hashes and that each chunk's original offset, size, and SHA-256 match. The archives themselves were fully verified by PX1 before export.

| Observation | Result |
| --- | ---: |
| Combined smaller than v1.1 Best Size | 145 chunks |
| Combined larger than v1.1 Best Size | 30 chunks |
| Gross savings on winning chunks | 6,324,424 bytes |
| Total increase on losing chunks | 313 bytes |
| Net compressed-chunk savings | 6,324,111 bytes |
| Median saving per chunk | 42,404 bytes |
| Largest chunk saving | 78,872 bytes, chunk 87 |
| Largest chunk increase | 12 bytes, chunk 22 |
| Combined smaller than level-19 zstd | 175 of 175 chunks |

The archive-level saving versus v1.1 is 6,324,109 bytes, two bytes less than the payload saving because of the slightly larger v1.2 manifest. Run `python scripts/export_compression_results.py ZSTD19.px1 BEST_SIZE.px1 COMBINED.px1 OUTPUT.csv` to regenerate the CSV from other verified PX1 files made from the same source disc.
