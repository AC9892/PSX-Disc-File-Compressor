# PX1 Disc Compressor

PX1 is a Windows desktop utility that compresses one PlayStation BIN/CUE disc into a `.px1` file, verifies it, and restores the original BIN and CUE byte for byte. It uses Tauri 2, TypeScript/HTML/CSS, Rust, zstd, and SHA-256.

## What v0.1 supports

- One CUE referencing one adjacent `BINARY` BIN with 2352-byte sectors. The BIN may contain multiple tracks. Other CUE layouts are rejected clearly.
- Independently compressed 4 MiB BIN chunks, exact CUE bytes, per-chunk and original-file SHA-256 hashes, and a whole-container SHA-256 footer.
- Compress, Extract, and Verify desktop workflows with progress, cancellation, and a local log.
- Optional disc metadata from the image's data track: ISO 9660 volume ID, `SYSTEM.CNF` boot executable, PlayStation serial, and region when detectable. Title and disc number can be entered as descriptions.

Source filenames are filesystem labels, not evidence of disc identity. PX1 never infers a game title, region, serial, or disc number from them. Original BIN/CUE bytes and their SHA-256 hashes are authoritative. The app reads source files without modifying them, writes temporary outputs, and refuses to overwrite existing destinations.

`FILE` names in the CUE must be UTF-8 so an undecodable label cannot select the wrong BIN. UTF-8 CUEs with a byte-order mark are accepted; non-UTF-8 bytes elsewhere in a CUE are preserved exactly. Multi-BIN layouts, CHD conversion, `.px1md`, and direct emulator loading are outside v0.1.

## Build on Windows

Install Node.js, Rust with the MSVC toolchain, and the Windows build tools required by Tauri 2. From this folder:

```powershell
npm ci
npm run tauri dev
```

The frontend is built into `dist` and loaded by the Tauri window. PX1 uses Tauri IPC to call its standalone Rust core; it does not start a localhost server or expose an HTTP API. The app itself makes no network requests.

Build the Windows executable and offline MSI/NSIS installers with:

```powershell
npm run tauri build
```

The installers appear under `src-tauri/target/release/bundle/` and include the offline WebView2 installer, so installation does not require internet. Building from a fresh checkout may require internet to obtain npm/Rust dependencies and the WebView2 installer. The direct executable at `src-tauri/target/release/px1-disc-compressor.exe` uses an already installed WebView2 runtime.

## Tests and format

Run the regular synthetic-disc tests:

```powershell
cargo test --manifest-path px1-core/Cargo.toml
```

Run the slower, roughly 700 MiB synthetic round-trip test separately:

```powershell
cargo test --manifest-path px1-core/Cargo.toml full_size_disc_streams_and_roundtrips -- --ignored
```

`px1-core` has no Tauri or network dependency. Its `compression` module handles zstd chunks without PlayStation sector assumptions; CUE, metadata, and container modules provide the PX1-specific rules. The documented versioned container is in [docs/PX1_FORMAT.md](docs/PX1_FORMAT.md).

## Upload this repository

Git tracks source code, lockfiles, the icon, and documentation. Build outputs, installers, temporary files, and disc images are ignored. To publish to a new remote repository after creating it on your Git host:

```powershell
git remote add origin <your-repository-url>
git push -u origin main
```

Share built installers separately as release assets rather than committing them to the source repository. No real disc image is included in this project.
