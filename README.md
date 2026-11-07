# PSX Disc File Compressor

PSX Disc File Compressor is a Windows desktop utility that compresses PS1 BIN/CUE discs into `.px1`, PS2 ISO or single-BIN CUE discs into CHD, and PS3 ISO images into `.ps3z`. It verifies archives and restores original files with byte-level checks where original hashes are available. The PS1 archive format uses the PX1 name. It uses Tauri 2, TypeScript/HTML/CSS, Rust, zstd, XZ, Brotli, and SHA-256.

> **Disc support in v0.1:** One CUE referencing one `BINARY` BIN made of 2352-byte sectors. Multiple tracks within that single BIN are supported. Multi-BIN discs and other CUE layouts are not supported yet.

## PS2 CHD support (development build)

The shared **Compress**, **Extract**, and **Verify** tabs handle PS1 PX1 and PS2 CHD. Selecting a CUE or ISO inspects disc contents and selects the matching workflow. PS2 support covers 2048-byte-sector DVD ISO images and single-BIN, 2352-byte-sector CD CUE images. PS2 is identified from `SYSTEM.CNF` `BOOT2`, not the filename. Unidentified images show **Unknown PlayStation Disc** and allow manual platform selection. ISO 9660 volume ID and boot serial are shown when readable. PS1 BIN/CUE stays on the original PX1 code path.

Install [`chdman` from MAME](https://docs.mamedev.org/tools/chdman.html) and place `chdman.exe` beside the portable app EXE, or put its folder on `PATH`. PS2 DVD ISO uses `createdvd` / `extractdvd`; PS2 CD BIN/CUE uses `createcd` / `extractcd`. `chdman verify` runs after creation. A temporary extraction is compared against the original SHA-256 hashes. The app stores original filenames, hashes, and exact CUE bytes in an adjacent `.chd.json` file; **keep this sidecar with the CHD**. The source images are never deleted or modified. The batch button handles multiple selected ISO/CUE images sequentially and logs each result.

CHD integrity alone does not prove exact source bytes. The app reports exact restoration only when extracted ISO or BIN/CUE bytes match the recorded original hashes. A CHD without this app's sidecar can be verified and extracted, but its original filenames and bytes cannot be proven. CHD conversion may normalize CD layouts; a mismatch is reported and the source should be retained. PS2 multi-BIN CUE is not supported.

## PS3 ISO support (development build)

PS3 support accepts a single, complete 2048-byte-sector Blu-ray ISO with the `PlayStation3` disc marker and an ISO 9660 primary volume descriptor. Detection reads disc contents, never the filename. It writes a separate `.ps3z` archive using streaming zstd compression; PX1 and CHD formats are unchanged. The archive contains the original ISO filename, byte length, and SHA-256. The app decodes and hashes the entire archive after compression. Verify repeats that check; Extract writes the original ISO filename and checks its bytes against the recorded hash. The source ISO is never modified or deleted. Compression and extraction can be cancelled; existing output files are never overwritten.

PS3Z is an application-specific archive, **not a bootable ISO or a CHD**. Extract it to an ISO before using another tool. It preserves the supplied ISO bytes exactly; it does not decrypt, repair, or validate a PS3 disc dump, nor does it claim the restored image will boot. The app reads a PS3 title from `PS3_GAME/PARAM.SFO` when present and a title ID from `PS3_DISC.SFB` or `PARAM.SFO`. The current PS3Z header does not retain those descriptive fields, so reopening a PS3Z shows limited game information. Folder-form dumps, split ISOs, encrypted-disc handling beyond byte preservation, and PS3 batch operations are not yet supported. A disc without the checked markers is refused rather than guessed to be PS3. Already compressed or encrypted data may shrink little. Keep original dumps until you independently test restoration and any desired gameplay.

The available 4,463,853,568-byte PS3 ISO passed compression, Verify, extraction, and a final source-versus-restored SHA-256 comparison. Its PS3Z archive was 4,356,276,113 bytes (97.59% of source; about 102.6 MiB saved). This verifies exact file restoration for that dump, not gameplay or compatibility with every PS3 disc. The [PS3Z format description](docs/PS3Z_FORMAT.md) documents the archive layout.

## Artwork

The right-side game panel shows metadata from the selected disc or archive, with a manual title/console option when identification is incomplete. Settings has its own tab and can hide the panel or disable artwork and optional identification. PS1 and PS2 standard covers match a separate local WebP collection by detected serial. PS3 title IDs come from disc metadata and match a separate local library of 3,809 GameTDB front covers; a missing cover can also be downloaded on demand, converted to WebP quality 85, and cached for offline use. Automatic PS3 downloads are off by default. Clear Artwork Cache removes individually downloaded PS3 covers and manual replacements after confirmation, while retaining all three platform libraries. Artwork is never stored in disc archives or embedded in the EXE. See [artwork setup, sources, and permissions](docs/ARTWORK.md).

If a PS3 dump includes a CUE with one `FILE "...ISO" BINARY` entry, selecting the CUE opens its referenced ISO after validating PS3 disc markers. PS3Z archives the ISO bytes only; keep the CUE separately. A manual PS3 choice is also available for an ISO routed to the unknown-disc screen, but it still requires successful disc-content validation.

The retained local collection has 9,118 PS1 standard covers, 10,430 PS2 standard covers, and 3,810 PS3 covers. WebP covers and thumbnails total 1,502,884,884 bytes (about 1.40 GiB). The original PS1/PS2 bulk conversion also processed 15,040 3D images, but those generated covers and thumbnails were removed after the user chose standard covers only. No image conversions failed. Two PS2 standard images had non-serial filenames and remain in `ps2/unmatched/default` without automatic game matching. The converted images and temporary clones are not in GitHub.

With MAME `chdman` 0.289, synthetic PS2 DVD ISO and single-BIN CD CUE images both passed CHD creation, verification, extraction, and original-file SHA-256 comparison. A full Dragon Ball Z: Budokai Tenkaichi 3 DVD ISO also passed: 3,020,816,384 source bytes became a 1,751,727,635-byte CHD (57.99% of source size; 1.182 GiB saved), and the explicit restored ISO matched the source hash. This is one real DVD test, not a claim of compatibility with every PS2 dump.

For PS2 CD conversion, the BIN must be beside the CUE with the exact filename in its `FILE` line. A manually relocated or renamed BIN remains supported by the PS1 PX1 workflow, but cannot yet be staged for PS2 CHD creation. Select the CUE rather than the BIN. The app also refuses to overwrite any existing output.

To test with your own PS2 dump: put `chdman.exe` beside the portable app or on `PATH`, open **Compress**, and select the Budokai Tenkaichi 3 ISO. The app switches to CHD output after detecting PS2. Choose a new CHD output path with enough free space for the CHD plus a temporary extracted ISO. After compression, confirm the result says exact restoration verified. Open the CHD on **Verify**, then **Extract** it to an empty folder. Compare the restored ISO SHA-256 with the original. Repeat with a single-BIN PS2 CD CUE, then select multiple ISO/CUE images with **Batch ISO/CUE** and review each log entry. A CHD created elsewhere can be inspected, verified, and extracted, but without original hashes the app reports its exactness as unproven.

## What v0.1 supports

- One CUE referencing one `BINARY` BIN with 2352-byte sectors. The BIN may contain multiple tracks. PX1 locates it from the CUE by default; if the BIN was renamed or moved, the user can select it manually. Other CUE layouts are rejected clearly.
- Independently compressed 4 MiB BIN chunks, exact CUE bytes, per-chunk and original-file SHA-256 hashes, and a whole-container SHA-256 footer.
- Fast, Balanced, and Maximum use zstd. Best Size tries zstd, XZ, and reversible byte-delta variants on each chunk, then stores the smallest. Best Size takes longer but is still lossless; its actual savings depend on the disc.
- Combine Best Two applies Brotli and XZ to every chunk, trying both orders and storing the shorter two-layer result. These were the two smallest individual codecs in a 12-chunk sample spread across the test disc. Applying two codecs can be larger than using the strongest one alone, so compare the final PX1 sizes. Extraction reverses both layers and verifies the original BIN/CUE hashes.
- Compress, Extract, and Verify desktop workflows with progress, cancellation, and a local log.
- Optional disc metadata from the image's data track: ISO 9660 volume ID, `SYSTEM.CNF` boot executable, PlayStation serial, and region when detectable. Title and disc number can be entered as descriptions.

Source filenames are filesystem labels, not evidence of disc identity. PX1 never infers a game title, region, serial, or disc number from them. Original BIN/CUE bytes and their SHA-256 hashes are authoritative. The app reads source files without modifying them, writes temporary outputs, and refuses to overwrite existing destinations.

When a manually selected BIN has a different filename from the CUE's `FILE` entry, the PX1 records the selected source filename as optional metadata. Extraction names the restored BIN as the CUE expects, so the untouched CUE can locate it. The source BIN is not renamed or modified.

`FILE` names in the CUE must be UTF-8 so an undecodable label cannot select the wrong BIN. UTF-8 CUEs with a byte-order mark are accepted; non-UTF-8 bytes elsewhere in a CUE are preserved exactly. Multi-BIN layouts, `.px1md` import, and direct emulator loading of PX1 or PS3Z are outside v0.1.

## Build on Windows

Install Node.js, Rust with the MSVC toolchain, and the Windows build tools required by Tauri 2. From this folder:

```powershell
npm ci
npm run tauri dev
```

The frontend is built into `dist` and loaded by the Tauri window. PX1 uses Tauri IPC to call its standalone Rust core; it does not start a localhost server or expose an HTTP API. Disc compression works offline. PS3 cover downloads, when requested or enabled in Settings, use outbound HTTPS.

Build the Windows executable and offline MSI/NSIS installers with:

```powershell
npm run tauri build
```

The installers appear under `src-tauri/target/release/bundle/` and include the offline WebView2 installer, so installation does not require internet. Building from a fresh checkout may require internet to obtain npm/Rust dependencies and the WebView2 installer. The direct executable at `src-tauri/target/release/px1-disc-compressor.exe` uses an already installed WebView2 runtime.

For a portable EXE without machine-specific build paths, run `powershell -ExecutionPolicy Bypass -File scripts/build_portable.ps1`. This builds without installers, remaps local source and Cargo paths, checks the EXE for local paths, and copies it to `src-tauri/target/release/bundle/portable/`.

The Windows app icon is generated from `src-tauri/icons/source.png` and committed as `src-tauri/icons/icon.ico`. To regenerate it after changing the artwork, install Pillow in Python and run `python scripts/make_icon.py` before building.

## Tests and format

Run the regular synthetic-disc tests:

```powershell
cargo test --manifest-path px1-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Run the slower, roughly 700 MiB synthetic round-trip test separately:

```powershell
cargo test --manifest-path px1-core/Cargo.toml full_size_disc_streams_and_roundtrips -- --ignored
```

`px1-core` has no Tauri or network dependency. Its `compression` module handles zstd, XZ, and Brotli chunks without PlayStation sector assumptions; CUE, metadata, and container modules provide the PX1-specific rules. The app reads PX1 v1.0, Best Size v1.1, and two-layer v1.2 archives. See the [format specification](docs/PX1_FORMAT.md) and [codec benchmark](docs/COMPRESSION_BENCHMARK.md).

On Windows, if tests fail with access errors in the system temporary folder, set `TEMP` and `TMP` to a writable test folder before running Cargo. The optional full PS3 ISO round trip requires `PS3_TEST_ISO` (source path), `PS3_FULL_TEST=1`, and `PS3_TEST_WORKDIR` (a separate folder with at least 10 GiB free). It creates disposable archive and restored files there and removes them after a successful comparison. The source ISO is never changed.

## Upload this repository

Git tracks source code, lockfiles, the icon, and documentation. Build outputs, installers, temporary files, and disc images are ignored. To publish to a new remote repository after creating it on your Git host:

```powershell
git remote add origin <your-repository-url>
git push -u origin main
```

Share built installers separately as release assets rather than committing them to the source repository. No real disc image is included in this project.
