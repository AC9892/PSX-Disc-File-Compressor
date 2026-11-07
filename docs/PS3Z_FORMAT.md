# PS3Z ISO archive, version 1

PS3Z is a single-file, lossless archive for one PS3 disc ISO. It is not a disc image that an emulator can mount directly. The current reader and writer are in `src-tauri/src/ps3.rs`. All integers are little endian.

| Offset | Bytes | Field |
| --- | ---: | --- |
| 0 | 8 | ASCII magic `PS3ZISO1` |
| 8 | 8 | Original ISO length (`u64`) |
| 16 | 2 | UTF-8 filename length (`u16`, 1–1024) |
| 18 | 32 | SHA-256 of the exact original ISO bytes |
| 50 | variable | Original filename, UTF-8, without any directory path |
| 50 + filename length | remaining | Zstandard-compressed ISO byte stream |

The writer creates one zstd stream at level 9. It hashes the source before writing, checks the source again after writing, decodes the new archive and compares decoded size and SHA-256, then moves the completed temporary archive to the chosen output name. It refuses to replace an existing archive. Verify decodes and hashes the complete stream without writing an ISO. Extract writes the original filename to an existing output folder and checks decoded size and hash. Failed or cancelled extraction removes its incomplete output.

The stored hash proves that the decoded bytes match the bytes supplied to the writer, assuming the archive header is trusted. The format does not authenticate who created an archive and does not add a separate whole-container checksum. PS3Z does not decrypt, repair, or judge playability of a disc image. The original ISO should be retained until independent restoration and gameplay tests are complete.
