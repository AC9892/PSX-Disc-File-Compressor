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
