# PX1 format version 1

PX1 stores one PlayStation BIN/CUE disc without changing either source file. It is a binary container with a UTF-8 JSON manifest, the exact CUE bytes, an indexed sequence of independently compressed BIN chunks, and a whole-container SHA-256 footer. All integers in the binary structures are little endian. Offsets are absolute byte offsets from the start of the file. Version 1 supports one CUE that references one BINARY BIN file; the BIN may contain multiple tracks. The BIN can be selected manually when its current path no longer matches the CUE's `FILE` entry.

## File layout

| Section | Location | Size |
| --- | --- | --- |
| Header | 0 | 96 bytes |
| Manifest | `manifest_offset` | `manifest_length` bytes |
| Original CUE | `cue_offset` | `cue_length` bytes |
| Chunk table | `table_offset` | `table_length` bytes |
| Compressed BIN chunks | `data_offset` | through `footer_offset - 1` |
| Container SHA-256 | `footer_offset` | 32 bytes |

Sections must be contiguous in that order, with no padding. The footer is the raw SHA-256 digest of every byte before the footer, including the final header and chunk table. The file ends immediately after the footer.

## Header

| Offset | Width | Field | Version 1 value or meaning |
| ---: | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `PX1` followed by `00` (`50 58 31 00`) |
| 4 | 2 | Major version | `1` |
| 6 | 2 | Minor version | `0` (zstd) or `1` (Best Size) |
| 8 | 4 | Header size | `96` |
| 12 | 4 | Flags | `0` |
| 16 | 8 | Manifest offset | `96` |
| 24 | 8 | Manifest length | UTF-8 JSON length, at most 1 MiB |
| 32 | 8 | CUE offset | Immediately after manifest |
| 40 | 8 | CUE length | Original CUE byte count, at most 8 MiB |
| 48 | 8 | Chunk table offset | Immediately after CUE |
| 56 | 8 | Chunk table length | `chunk_count × 56` for v1.0, `chunk_count × 60` for v1.1 |
| 64 | 8 | Data offset | Immediately after chunk table |
| 72 | 8 | Chunk count | Number of BIN chunks |
| 80 | 8 | Footer offset | Immediately after last compressed chunk |
| 88 | 4 | Compression method ID | `1` = independent zstd frames for v1.0; `2` = per-chunk codec IDs for v1.1 |
| 92 | 4 | Uncompressed chunk size | `4,194,304` bytes |

Readers reject unsupported major/minor versions, nonzero unknown flags, unknown compression IDs, invalid section bounds, and inconsistent counts. The v1.1 chunk table adds a codec ID while retaining the other v1 fields.

## Manifest

The manifest is UTF-8 JSON. Required fields:

```json
{
  "format_version": 1, "format_minor": 0,
  "created_utc": "2026-10-07T22:00:00Z",
  "bin": {"name": "20261007_021434.BIN", "size": 732271680, "sha256": "64 lowercase hexadecimal characters"},
  "cue": {"name": "20261007_021434.CUE", "size": 83, "sha256": "64 lowercase hexadecimal characters"},
  "disc": {
    "platform": "PlayStation", "media": "CD-ROM", "serial": "SLUS-00892",
    "region": "USA", "volume_id": "EXAMPLE", "boot_executable": "cdrom:\\SLUS_008.92;1",
    "title": null, "disc_number": null
  },
  "compression": "zstd", "zstd_level": 6,
  "chunk_size": 4194304, "chunk_count": 175
}
```

`bin.name` and `cue.name` are safe filenames used for extraction. `bin.name` is the CUE's `FILE` label, so the restored BIN can be found by the unchanged CUE. If the user manually selected a BIN with a different current filename, `bin.source_name` optionally records that source label as descriptive metadata. These names must not establish disc identity. Extraction names must be single safe filenames, without directory components, common Windows reserved device names, trailing spaces or dots, or Windows-invalid characters. The CUE byte sequence is stored separately and is never regenerated. The two file hashes are SHA-256 of the original complete files. `disc` fields can be null when unknown. A detected serial comes from the boot executable in `SYSTEM.CNF` on an ISO 9660 data track. User entered title and disc number are descriptive only; they cannot change the stored bytes or hashes. Unknown JSON fields may be ignored by a compatible reader.

For Best Size archives, `format_minor` is `1` and `compression` is `"mixed"`. Older v1.0 manifests may omit `format_minor`; readers treat it as zero. `zstd_level` records the level used for zstd candidates; Best Size currently uses level 19. XZ candidates use preset 6.

## Chunk table

Each v1.0 entry is 56 bytes. Each v1.1 entry is 60 bytes:

| Offset within entry | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | Uncompressed BIN offset |
| 8 | 8 | Compressed PX1 file offset |
| 16 | 4 | Uncompressed size |
| 20 | 4 | Compressed size |
| 24 | 32 | Raw SHA-256 of uncompressed chunk |
| 56 | 4 | v1.1 only: chunk codec ID |

Entries are ordered. Uncompressed offsets start at zero and are contiguous. Compressed offsets start at `data_offset` and are contiguous. Every chunk except the last is exactly 4 MiB uncompressed; the last may be shorter. Version 1 requires a nonempty BIN whose total length is divisible by 2352. Each compressed range is independent, permitting random chunk reads in a future emulator integration. Readers must limit decompressed output to the recorded uncompressed size.

For v1.0, every chunk is a zstd frame and has no codec ID. For v1.1, the codec ID is one of:

| ID | Pipeline |
| ---: | --- |
| 1 | zstd frame of original bytes |
| 2 | XZ stream of original bytes |
| 3 | zstd frame of bytewise deltas |
| 4 | XZ stream of bytewise deltas |

The delta transform encodes each byte as `(current - previous) mod 256`, with `previous = 0` at the start of each chunk. Decoding cumulatively adds deltas modulo 256. Best Size encodes all four candidates per chunk and stores the shortest result. The original chunk hash is always computed from the bytes before any transform. Stacking zstd and XZ on the same bytes is not used; the codecs are alternative candidates.

## Validation and extraction

1. Validate magic, version, header fields, file size, offsets, manifest, safe output names, and chunk table continuity.
2. Hash the stored CUE bytes and compare to `cue.sha256`.
3. Decode each chunk independently according to its codec ID (or zstd for v1.0), reversing the delta transform when present. Check its length and SHA-256. Hash the concatenated original bytes and compare to `bin.sha256`.
4. Hash all bytes before the footer and compare to the raw footer digest.
5. Extract to new temporary files in the output directory; compare resulting complete file hashes before renaming. Existing output files must not be overwritten.

The current writer verifies its own temporary PX1 before renaming it to `.px1`. Extraction verifies the container first, then verifies extracted bytes again. A failed operation removes its temporary files. Source BIN/CUE files are read only.

## Compatibility

Current PX1 readers accept both v1.0 and v1.1. Older readers that support only v1.0 will reject Best Size archives. Readers must reject unknown binary flags, compression methods, and chunk codec IDs unless explicitly implemented. The format has no dependency on CHD, chdman, or Tauri. A future `.px1md` game manifest may reference individual PX1 disc files; it is outside version 1.
