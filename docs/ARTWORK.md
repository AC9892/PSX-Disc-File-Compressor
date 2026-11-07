# Local artwork

Artwork is optional and never changes a disc image, PX1, CHD, or PS3Z archive. **Settings → Game Information & Artwork** controls the right-side information panel, detection, cover display, downloads, and caching. The **Artwork** disclosure and Settings show the separate data folder and its total WebP size. The portable app finds an existing `artwork` collection beside the EXE or in its source checkout. Otherwise it uses the operating system's application data directory. You can always choose another folder in Settings. The collection is ignored by Git and is not bundled into the EXE. Only flat covers are supported, as requested.

## Folder layout

```text
artwork/
  ps1/default/<serial>.webp
  ps2/default/<serial>.webp
  ps3/library/covers/<title-id>.webp
  ps3/covers/<title-id>.webp  (individual downloads)
  thumbnails/<platform>/<style>/<id>.webp
  conversion-report.json
  ATTRIBUTION.md
```

Local PS1/PS2 matching uses the serial read from disc contents or stored archive metadata. Filename guesses are never used. Standard covers appear beside the disc information in Compress, Extract, and Verify. Click a thumbnail to view the retained full-resolution WebP. A user-selected replacement is stored under `<platform>/manual/default/<serial>.webp` and takes precedence over the collection. **Remove cached artwork** removes manual replacements or individually downloaded covers; it leaves the separate PS3 library. Missing art is shown as unavailable without affecting compression.

Turning off **Prefer locally stored artwork** hides the PS1/PS2 collection while retaining manual replacements. PS3 still reuses an already cached image, because downloading and converting that same image again would waste bandwidth and storage. Turning off artwork skips cover lookup and decoding; turning off caching prevents automatic PS3 downloads.

PS3 matching reads the title ID from `PS3_DISC.SFB` or `PS3_GAME/PARAM.SFO` inside the ISO. The user may enter an artwork ID manually if the disc metadata is unreadable; that descriptive choice never changes the ISO or archive. Automatic downloads are off by default; the Settings button downloads a missing cover for the selected identified PS3 game. The app uses the separate offline library first, then requests a single missing cover from GameTDB using a likely region (JA for Japanese title IDs, US for US IDs, EN otherwise) and a small fallback list. It caches the converted WebP and thumbnail. A missing result is cached for seven days to avoid repeated requests. The request is outbound HTTPS; the app does not start a server or require a login or key. Existing covers, disc tools, and cached artwork work without internet. **Clear Artwork Cache** deletes individually downloaded PS3 covers and manual replacements after confirmation; it retains all three platform libraries. [GameTDB](https://www.gametdb.com/PS3) permits artwork use in software when the software links back to the site; the app includes that link. GameTDB asks developers to contact them about projects; no message has been sent on the user's behalf.

The converter is `scripts/process_artwork.py`. It requires Pillow with WebP support and accepts `--ps1`, `--ps2`, and `--output` paths. It processes only `covers/default`, writes WebP quality 85, preserves any transparency, creates thumbnails no larger than 160 × 160 without upscaling, and records image counts, input/output sizes, exact duplicate source hashes, unmatched filenames, and conversion failures. It checks free space before the bulk run. Existing converted files are reused. The original clones are read only during conversion.

The source repositories are [xlenore/psx-covers](https://github.com/xlenore/psx-covers) and [xlenore/ps2-covers](https://github.com/xlenore/ps2-covers). Neither declares a license for the collected artwork. The local converted collection is not redistributable as part of this app without permission from the relevant rights holders. The conversion report and local `ATTRIBUTION.md` retain source links and the credits stated by the repositories.

The retained local collection has 9,118 PS1 standard covers, 10,430 PS2 standard covers, and 3,810 PS3 covers (3,809 from the regional packs plus one previously cached cover). Two PS2 standard images have filenames that cannot be matched safely to a serial, so they are kept under `ps2/unmatched/default`. The initial PS1/PS2 bulk conversion found 6,809 exact duplicate source hashes and no failures. Its 15,040 3D images were removed at the user's request. The temporary Git clones were removed after the output and report were checked.

The PS3 library came from all 21 flat front-cover packs on [GameTDB's PS3 Downloads page](https://www.gametdb.com/PS3/Downloads), snapshot 2026-10-28. The packs contain 6,124 source images (470,684,687 bytes); repeated title IDs across regions were skipped. The 3,809 newly imported WebP covers and thumbnails total 183,084,338 bytes, about 61% less than the source images. No conversion failures occurred. `artwork/ps3-import-report.json` has per-region counts. `scripts/import_ps3_packs.ps1` and the Rust converter can repeat the import from downloaded packs. The original ZIPs remain in Downloads. The local artwork is not included in Git or the executable. GameTDB's [FAQ](https://www.gametdb.com/Main/FAQ) permits artwork use in software with a site link and asks developers to contact them about projects; no contact was sent on the user's behalf.
