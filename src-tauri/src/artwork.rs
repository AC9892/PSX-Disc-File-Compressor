use base64::{engine::general_purpose::STANDARD, Engine};
use image::{imageops, ImageReader};
use serde::Serialize;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

#[derive(Serialize)]
pub struct ArtworkStats {
    pub files: u64,
    pub bytes: u64,
}

/// Find a separate cover collection beside a portable executable or this source checkout.
/// This only selects an existing directory; it does not move or modify artwork.
pub fn nearby_collection(executable: &Path) -> Option<PathBuf> {
    for ancestor in executable.parent()?.ancestors().take(6) {
        if ancestor.parent().is_none() { continue; }
        let folder = ancestor.join("artwork");
        if folder.join("ps1/default").is_dir() && folder.join("ps2/default").is_dir() {
            return Some(folder);
        }
    }
    None
}

fn valid_id(value: &str) -> Result<String, String> {
    let id = value.to_ascii_uppercase();
    if !(4..=24).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err("Invalid disc serial or title ID".into());
    }
    Ok(id)
}
fn kind(platform: &str, variant: &str) -> Result<(&'static str, &'static str), String> {
    let p = match platform {
        "ps1" => "ps1",
        "ps2" => "ps2",
        "ps3" => "ps3",
        _ => return Err("Unsupported artwork platform".into()),
    };
    let v = match variant {
        "default" | "covers" => {
            if p == "ps3" {
                "covers"
            } else {
                "default"
            }
        }
        _ => return Err("Unsupported cover style".into()),
    };
    Ok((p, v))
}
fn local_path(
    root: &Path,
    platform: &str,
    variant: &str,
    id: &str,
    thumb: bool,
    manual: bool,
) -> PathBuf {
    let mut p = root.to_path_buf();
    if thumb {
        p.push("thumbnails");
    }
    p.push(platform);
    if manual {
        p.push("manual");
    }
    p.push(variant);
    p.join(format!("{id}.webp"))
}
pub fn lookup(
    root: &Path,
    platform: &str,
    variant: &str,
    serial: &str,
    thumbnail: bool,
) -> Result<Option<String>, String> {
    lookup_with_source(root, platform, variant, serial, thumbnail, false)
}
pub fn lookup_with_source(
    root: &Path,
    platform: &str,
    variant: &str,
    serial: &str,
    thumbnail: bool,
    manual_only: bool,
) -> Result<Option<String>, String> {
    let id = valid_id(serial)?;
    let (platform, variant) = kind(platform, variant)?;
    let paths = [
        (local_path(root, platform, variant, &id, thumbnail, true), true),
        (local_path(root, platform, variant, &id, thumbnail, false), false),
        (library_path(root, platform, variant, &id, thumbnail), false),
        (local_path(root, platform, variant, &id, false, true), true),
        (local_path(root, platform, variant, &id, false, false), false),
        (library_path(root, platform, variant, &id, false), false),
    ];
    for (path, manual) in paths {
        if manual_only && !manual { continue; }
        if path.is_file() {
            if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
                return Err("Artwork file exceeds 8 MiB".into());
            }
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            return Ok(Some(format!(
                "data:image/webp;base64,{}",
                STANDARD.encode(bytes)
            )));
        }
    }
    Ok(None)
}
fn library_path(root: &Path, platform: &str, variant: &str, id: &str, thumb: bool) -> PathBuf {
    let mut path = root.to_path_buf();
    if thumb { path.push("thumbnails"); }
    path.push(platform);
    path.push("library");
    path.push(variant);
    path.join(format!("{id}.webp"))
}
fn webp_pair(source: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    if source.len() > 12 * 1024 * 1024 {
        return Err("Artwork source exceeds 12 MiB".into());
    }
    let reader = ImageReader::new(Cursor::new(source))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err("Artwork dimensions exceed 4096 pixels".into());
    }
    let rgba = image::load_from_memory(source)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let full = webp::Encoder::from_rgba(rgba.as_raw(), w, h)
        .encode(85.0)
        .to_vec();
    let thumb = imageops::thumbnail(&rgba, w.min(160), h.min(160));
    let preview = webp::Encoder::from_rgba(thumb.as_raw(), thumb.width(), thumb.height())
        .encode(85.0)
        .to_vec();
    if full.is_empty() || preview.is_empty() {
        return Err("WebP encoding failed".into());
    }
    Ok((full, preview))
}
fn write_pair(
    root: &Path,
    platform: &str,
    variant: &str,
    id: &str,
    source: &[u8],
    manual: bool,
) -> Result<(), String> {
    let (full, thumb) = webp_pair(source)?;
    let paths = [
        (local_path(root, platform, variant, id, false, manual), full),
        (local_path(root, platform, variant, id, true, manual), thumb),
    ];
    for (path, bytes) in paths {
        fs::create_dir_all(path.parent().ok_or("Invalid artwork directory")?)
            .map_err(|e| e.to_string())?;
        fs::write(&path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn replace(
    root: &Path,
    platform: &str,
    variant: &str,
    serial: &str,
    selected: &Path,
) -> Result<(), String> {
    let id = valid_id(serial)?;
    let (platform, variant) = kind(platform, variant)?;
    let bytes = fs::read(selected).map_err(|e| e.to_string())?;
    write_pair(root, platform, variant, &id, &bytes, true)
}
pub fn remove(root: &Path, platform: &str, variant: &str, serial: &str) -> Result<(), String> {
    let id = valid_id(serial)?;
    let (platform, variant) = kind(platform, variant)?;
    for manual in [true, false] {
        if platform != "ps3" && !manual {
            continue;
        }
        for thumb in [true, false] {
            let path = local_path(root, platform, variant, &id, thumb, manual);
            if path.is_file() {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
    }
    if platform == "ps3" {
        let marker = root
            .join("ps3")
            .join("covers")
            .join(format!("{id}.missing"));
        if marker.is_file() {
            fs::remove_file(marker).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn tally(root: &Path, stats: &mut ArtworkStats) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    for item in fs::read_dir(root).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let kind = item.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            tally(&item.path(), stats)?;
        } else if kind.is_file()
            && item
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("webp"))
        {
            stats.files += 1;
            stats.bytes += item.metadata().map_err(|e| e.to_string())?.len();
        }
    }
    Ok(())
}
pub fn stats(root: &Path) -> Result<ArtworkStats, String> {
    let mut result = ArtworkStats { files: 0, bytes: 0 };
    tally(root, &mut result)?;
    Ok(result)
}
fn clear_files(folder: &Path, count: &mut u64) -> Result<(), String> {
    if !folder.is_dir() { return Ok(()); }
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() { continue; }
        if kind.is_dir() { clear_files(&entry.path(), count)?; }
        else if kind.is_file() && entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("webp") || e.eq_ignore_ascii_case("missing")) {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}
/// Clear downloaded PS3 images and manual overrides, retaining the PS1/PS2 collections.
pub fn clear_cache(root: &Path) -> Result<u64, String> {
    let mut count = 0;
    for platform in ["ps1", "ps2", "ps3"] {
        clear_files(&root.join(platform).join("manual"), &mut count)?;
        clear_files(&root.join("thumbnails").join(platform).join("manual"), &mut count)?;
    }
    clear_files(&root.join("ps3").join("covers"), &mut count)?;
    clear_files(&root.join("thumbnails").join("ps3").join("covers"), &mut count)?;
    Ok(count)
}
pub fn download_ps3(root: &Path, title_id: &str) -> Result<Option<String>, String> {
    let id = valid_id(title_id)?;
    if id.len() != 9
        || !id.bytes().take(4).all(|b| b.is_ascii_uppercase())
        || !id.bytes().skip(4).all(|b| b.is_ascii_digit())
    {
        return Err("Invalid PS3 title ID".into());
    }
    if let Some(cover) = lookup(root, "ps3", "covers", &id, true)? {
        return Ok(Some(cover));
    }
    let marker = root
        .join("ps3")
        .join("covers")
        .join(format!("{id}.missing"));
    if let Ok(meta) = fs::metadata(&marker) {
        if meta
            .modified()
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < Duration::from_secs(7 * 86400))
        {
            return Ok(None);
        }
    }
    let region = if id.starts_with("BLJM") || id.starts_with("BLJS") || id.starts_with("BCJS") {
        "JA"
    } else if id.starts_with("BLUS") || id.starts_with("BCUS") {
        "US"
    } else {
        "EN"
    };
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .https_only(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let options = if region == "EN" {
        vec![("EN", "jpg"), ("EN", "png")]
    } else {
        vec![
            (region, "jpg"),
            (region, "png"),
            ("EN", "jpg"),
            ("EN", "png"),
        ]
    };
    for (index, (region, ext)) in options.into_iter().enumerate() {
        if index > 0 {
            std::thread::sleep(Duration::from_millis(1100));
        }
        let url = format!("https://art.gametdb.com/ps3/cover/{region}/{id}.{ext}");
        match agent.get(&url).call() {
            Ok(mut response) => {
                let mut bytes = Vec::new();
                response
                    .body_mut()
                    .as_reader()
                    .take(12 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 12 * 1024 * 1024 {
                    return Err("GameTDB cover exceeds 12 MiB".into());
                }
                write_pair(root, "ps3", "covers", &id, &bytes, false)?;
                return lookup(root, "ps3", "covers", &id, true);
            }
            Err(ureq::Error::StatusCode(404)) => {}
            Err(ureq::Error::StatusCode(429)) => {
                return Err("GameTDB rate limit reached; try again later".into())
            }
            Err(error) => return Err(format!("GameTDB is unavailable: {error}")),
        }
    }
    fs::create_dir_all(marker.parent().ok_or("Invalid cache directory")?)
        .map_err(|e| e.to_string())?;
    fs::write(
        marker,
        b"No GameTDB cover found; retry after seven days or remove this cache marker.",
    )
    .map_err(|e| e.to_string())?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_and_paths_are_confined() {
        assert!(valid_id("SLUS-00892").is_ok());
        assert!(valid_id("..\\bad").is_err());
        assert!(kind("ps1", "3d").is_err());
        assert!(kind("ps3", "3d").is_err());
    }
    #[test]
    fn portable_build_finds_separate_artwork() {
        let root = std::env::temp_dir().join(format!("artwork-nearby-test-{}", std::process::id()));
        let exe = root.join("src-tauri/target/release/bundle/portable/app.exe");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("artwork/ps1/default")).unwrap();
        fs::create_dir_all(root.join("artwork/ps2/default")).unwrap();
        assert_eq!(nearby_collection(&exe), Some(root.join("artwork")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manual_cover_keeps_alpha_and_can_be_removed() {
        let root = std::env::temp_dir().join(format!("artwork-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let png = root.join("image.png");
        let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 30, 40, 90]));
        image.save(&png).unwrap();
        replace(&root, "ps1", "default", "SLUS-00892", &png).unwrap();
        let cover = local_path(&root, "ps1", "default", "SLUS-00892", false, true);
        let decoded = image::open(&cover).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (8, 8));
        assert!(decoded.get_pixel(0, 0)[3] < 255);
        assert!(lookup(&root, "ps1", "default", "SLUS-00892", true)
            .unwrap()
            .is_some());
        remove(&root, "ps1", "default", "SLUS-00892").unwrap();
        assert!(!cover.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn clearing_cache_keeps_the_standard_collection() {
        let root = std::env::temp_dir().join(format!("artwork-clear-test-{}", std::process::id()));
        fs::create_dir_all(root.join("ps1/default")).unwrap();
        fs::create_dir_all(root.join("ps1/manual/default")).unwrap();
        fs::create_dir_all(root.join("ps3/covers")).unwrap();
        fs::create_dir_all(root.join("ps3/library/covers")).unwrap();
        fs::write(root.join("ps1/default/SLUS-00892.webp"), b"collection").unwrap();
        fs::write(root.join("ps1/manual/default/SLUS-00892.webp"), b"manual").unwrap();
        fs::write(root.join("ps3/covers/BLJM85001.webp"), b"downloaded").unwrap();
        fs::write(root.join("ps3/library/covers/BLUS30196.webp"), b"library").unwrap();
        assert_eq!(clear_cache(&root).unwrap(), 2);
        assert!(root.join("ps1/default/SLUS-00892.webp").exists());
        assert!(root.join("ps3/library/covers/BLUS30196.webp").exists());
        assert!(lookup(&root, "ps3", "covers", "BLUS30196", false).unwrap().is_some());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manual_only_lookup_skips_collection() {
        let root = std::env::temp_dir().join(format!("artwork-manual-test-{}", std::process::id()));
        fs::create_dir_all(root.join("ps1/default")).unwrap();
        fs::write(root.join("ps1/default/SLUS-00892.webp"), b"collection").unwrap();
        assert!(lookup_with_source(&root, "ps1", "default", "SLUS-00892", false, true).unwrap().is_none());
        assert!(lookup(&root, "ps1", "default", "SLUS-00892", false).unwrap().is_some());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn optional_gametdb_cover() {
        if std::env::var_os("PS3_GAMETDB_TEST").is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!("gametdb-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        assert!(download_ps3(&root, "BLJM85001").unwrap().is_some());
        assert!(lookup(&root, "ps3", "covers", "BLJM85001", false)
            .unwrap()
            .is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
