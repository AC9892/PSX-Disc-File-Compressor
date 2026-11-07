use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Resolve a CUE that names one ISO. Identification still comes from inspect(),
/// never from the CUE or ISO filename. The CUE is not part of a PS3Z archive.
pub fn cue_iso_reference(cue: &Path) -> Result<Option<PathBuf>, String> {
    let len = fs::metadata(cue).map_err(fail)?.len();
    if len > 64 * 1024 { return Err("CUE is too large to inspect".into()); }
    let bytes = fs::read(cue).map_err(fail)?;
    let content = std::str::from_utf8(&bytes).map_err(|_| "CUE must be UTF-8")?.trim_start_matches('\u{feff}');
    let mut names = Vec::new();
    for line in content.lines() {
        let line = line.trim_start();
        if !line.get(..4).is_some_and(|s| s.eq_ignore_ascii_case("FILE")) || !line[4..].chars().next().is_some_and(char::is_whitespace) { continue; }
        let rest = line[4..].trim_start();
        let (name, tail) = if let Some(quoted) = rest.strip_prefix('"') {
            quoted.split_once('"').ok_or("Invalid quoted FILE entry in CUE")?
        } else {
            let name = rest.split_whitespace().next().ok_or("Missing FILE entry in CUE")?;
            (name, &rest[name.len()..])
        };
        if !tail.trim().eq_ignore_ascii_case("BINARY") { return Err("Unsupported CUE FILE type".into()); }
        names.push(name);
    }
    if names.len() != 1 { return Ok(None); }
    let name = names[0];
    if !name.to_ascii_lowercase().ends_with(".iso") { return Ok(None); }
    if name.is_empty() || name.ends_with([' ', '.']) || name.chars().any(|c| c.is_control() || "/\\:<>\"|?*".contains(c)) {
        return Err("Unsafe ISO filename in CUE".into());
    }
    let path = cue.parent().ok_or("CUE has no parent folder")?.join(name);
    if !path.is_file() { return Err(format!("ISO referenced by CUE is missing: {name}")); }
    Ok(Some(path))
}

const MAGIC: &[u8; 8] = b"PS3ZISO1";
const HEADER: u64 = 8 + 8 + 2 + 32;

#[derive(Debug, Clone, Serialize)]
pub struct Ps3Info {
    pub path: String,
    pub platform: String,
    pub media: String,
    pub format: String,
    pub original_name: String,
    pub original_size: u64,
    pub archive_size: Option<u64>,
    pub sha256: Option<String>,
    pub title_id: Option<String>,
    pub title: Option<String>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ps3Report {
    pub info: Ps3Info,
    pub verified: bool,
}

fn fail(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn safe_name(path: &Path) -> Result<String, String> {
    let s = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("Invalid filename")?;
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.ends_with([' ', '.'])
        || s.chars()
            .any(|c| c.is_control() || "/\\:<>\"|?*".contains(c))
    {
        return Err("Unsafe filename".into());
    }
    Ok(s.into())
}
fn digest(path: &Path, cancel: &AtomicBool) -> Result<[u8; 32], String> {
    let mut f = File::open(path).map_err(fail)?;
    let mut h = Sha256::new();
    let mut b = [0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Cancelled".into());
        }
        let n = f.read(&mut b).map_err(fail)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(h.finalize().into())
}
pub fn is_ps3_iso(path: &Path) -> Result<bool, String> {
    let mut f = File::open(path).map_err(fail)?;
    let len = f.metadata().map_err(fail)?.len();
    if len < 64 * 2048 || len % 2048 != 0 {
        return Ok(false);
    }
    let mut marker = [0u8; 12];
    f.seek(SeekFrom::Start(2048)).map_err(fail)?;
    f.read_exact(&mut marker).map_err(fail)?;
    if &marker != b"PlayStation3" {
        return Ok(false);
    }
    let mut pvd = [0u8; 6];
    f.seek(SeekFrom::Start(16 * 2048)).map_err(fail)?;
    f.read_exact(&mut pvd).map_err(fail)?;
    Ok(pvd == *b"\x01CD001")
}
fn disc_title_id(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut pvd = [0u8; 2048];
    file.seek(SeekFrom::Start(16 * 2048)).ok()?;
    file.read_exact(&mut pvd).ok()?;
    let root_sector = u32::from_le_bytes(pvd[158..162].try_into().ok()?) as u64;
    let root_size = u32::from_le_bytes(pvd[166..170].try_into().ok()?) as usize;
    if root_size == 0 || root_size > 65536 {
        return None;
    }
    let mut directory = vec![0u8; root_size];
    file.seek(SeekFrom::Start(root_sector.checked_mul(2048)?))
        .ok()?;
    file.read_exact(&mut directory).ok()?;
    let mut at = 0;
    while at < directory.len() {
        let length = directory[at] as usize;
        if length == 0 {
            at = (at / 2048 + 1) * 2048;
            continue;
        }
        if length < 34 || at + length > directory.len() {
            break;
        }
        let record = &directory[at..at + length];
        let name_length = record[32] as usize;
        if 33 + name_length <= length
            && record[33..33 + name_length].eq_ignore_ascii_case(b"PS3_DISC.SFB;1")
        {
            let sector = u32::from_le_bytes(record[2..6].try_into().ok()?) as u64;
            let size = u32::from_le_bytes(record[10..14].try_into().ok()?) as usize;
            if !(32..=65536).contains(&size) {
                return None;
            }
            let mut sfb = vec![0u8; size];
            file.seek(SeekFrom::Start(sector.checked_mul(2048)?)).ok()?;
            file.read_exact(&mut sfb).ok()?;
            if !sfb.starts_with(b".SFB") {
                return None;
            }
            for index in 0..sfb.len().saturating_sub(9) {
                let s = &sfb[index..];
                if s[..4].iter().all(u8::is_ascii_uppercase) {
                    let digits = if s.get(4) == Some(&b'-') {
                        s.get(5..10)
                    } else {
                        s.get(4..9)
                    };
                    if let Some(digits) = digits {
                        if digits.iter().all(u8::is_ascii_digit) {
                            return Some(format!(
                                "{}{}",
                                std::str::from_utf8(&s[..4]).ok()?,
                                std::str::from_utf8(digits).ok()?
                            ));
                        }
                    }
                }
            }
            return None;
        }
        at += length;
    }
    None
}
fn iso_entry(file: &mut File, extent: u64, bytes: usize, wanted: &str) -> Option<(u64, usize)> {
    if bytes == 0 || bytes > 1024 * 1024 { return None; }
    let mut data = vec![0u8; bytes];
    file.seek(SeekFrom::Start(extent.checked_mul(2048)?)).ok()?;
    file.read_exact(&mut data).ok()?;
    let mut at = 0;
    while at < bytes {
        let len = data[at] as usize;
        if len == 0 { at = (at / 2048 + 1) * 2048; continue; }
        if len < 34 || at + len > bytes { return None; }
        let row = &data[at..at + len];
        let name_len = row[32] as usize;
        if 33 + name_len <= len {
            let name = String::from_utf8_lossy(&row[33..33 + name_len]);
            if name.split(';').next()?.eq_ignore_ascii_case(wanted) {
                let sector = u32::from_le_bytes(row[2..6].try_into().ok()?) as u64;
                let size = u32::from_le_bytes(row[10..14].try_into().ok()?) as usize;
                return Some((sector, size));
            }
        }
        at += len;
    }
    None
}
fn disc_sfo(path: &Path) -> Option<(Option<String>, Option<String>)> {
    let mut file = File::open(path).ok()?;
    let mut pvd = [0u8; 2048];
    file.seek(SeekFrom::Start(16 * 2048)).ok()?;
    file.read_exact(&mut pvd).ok()?;
    if &pvd[..6] != b"\x01CD001" { return None; }
    let root = &pvd[156..190];
    let (dir_sector, dir_size) = iso_entry(&mut file,
        u32::from_le_bytes(root[2..6].try_into().ok()?) as u64,
        u32::from_le_bytes(root[10..14].try_into().ok()?) as usize,
        "PS3_GAME")?;
    let (sfo_sector, sfo_size) = iso_entry(&mut file, dir_sector, dir_size, "PARAM.SFO")?;
    if !(20..=1024 * 1024).contains(&sfo_size) { return None; }
    let mut sfo = vec![0u8; sfo_size];
    file.seek(SeekFrom::Start(sfo_sector.checked_mul(2048)?)).ok()?;
    file.read_exact(&mut sfo).ok()?;
    if &sfo[..4] != b"\0PSF" { return None; }
    let keys = u32::from_le_bytes(sfo[8..12].try_into().ok()?) as usize;
    let values = u32::from_le_bytes(sfo[12..16].try_into().ok()?) as usize;
    let count = u32::from_le_bytes(sfo[16..20].try_into().ok()?) as usize;
    if count > 256 { return None; }
    let mut title = None;
    let mut id = None;
    for n in 0..count {
        let row_start = 20 + n * 16;
        let row = sfo.get(row_start..row_start + 16)?;
        let key_at = keys.checked_add(u16::from_le_bytes(row[..2].try_into().ok()?) as usize)?;
        let key_tail = sfo.get(key_at..)?;
        let key_len = key_tail.iter().position(|&c| c == 0)?;
        let key = std::str::from_utf8(&key_tail[..key_len]).ok()?;
        if key != "TITLE" && key != "TITLE_ID" { continue; }
        let len = u32::from_le_bytes(row[4..8].try_into().ok()?) as usize;
        let value_at = values.checked_add(u32::from_le_bytes(row[12..16].try_into().ok()?) as usize)?;
        let raw = sfo.get(value_at..value_at.checked_add(len)?)?;
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        let value = std::str::from_utf8(&raw[..end]).ok()?.trim();
        if key == "TITLE" && !value.is_empty() && !value.chars().any(char::is_control) { title = Some(value.to_string()); }
        if key == "TITLE_ID" && value.len() == 9 && value.as_bytes()[..4].iter().all(u8::is_ascii_uppercase) && value.as_bytes()[4..].iter().all(u8::is_ascii_digit) { id = Some(value.to_string()); }
    }
    Some((title, id))
}
pub fn inspect(path: &Path) -> Result<Ps3Info, String> {
    if !is_ps3_iso(path)? {
        return Err("PS3 disc markers were not found in this ISO".into());
    }
    let sfo = disc_sfo(path);
    Ok(Ps3Info {
        path: path.display().to_string(),
        platform: "PlayStation 3".into(),
        media: "Blu-ray".into(),
        format: "ISO".into(),
        original_name: safe_name(path)?,
        original_size: fs::metadata(path).map_err(fail)?.len(),
        archive_size: None,
        sha256: None,
        title_id: disc_title_id(path).or_else(|| sfo.as_ref().and_then(|(_, id)| id.clone())),
        title: sfo.and_then(|(title, _)| title),
        confidence: "disc header and ISO 9660 filesystem".into(),
    })
}
fn read_header(f: &mut File, path: &Path) -> Result<(Ps3Info, [u8; 32]), String> {
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic).map_err(fail)?;
    if &magic != MAGIC {
        return Err("Invalid PS3Z archive header".into());
    }
    let mut n = [0u8; 8];
    f.read_exact(&mut n).map_err(fail)?;
    let original_size = u64::from_le_bytes(n);
    let mut n = [0u8; 2];
    f.read_exact(&mut n).map_err(fail)?;
    let name_len = u16::from_le_bytes(n) as usize;
    if name_len == 0 || name_len > 1024 {
        return Err("Invalid stored filename length".into());
    }
    let mut hash = [0u8; 32];
    f.read_exact(&mut hash).map_err(fail)?;
    let mut name = vec![0u8; name_len];
    f.read_exact(&mut name).map_err(fail)?;
    let name = String::from_utf8(name).map_err(fail)?;
    if safe_name(Path::new(&name))? != name {
        return Err("Unsafe stored filename".into());
    }
    let archive_size = fs::metadata(path).map_err(fail)?.len();
    if archive_size <= HEADER + name_len as u64 {
        return Err("Truncated PS3Z archive".into());
    }
    Ok((
        Ps3Info {
            path: path.display().to_string(),
            platform: "PlayStation 3".into(),
            media: "Blu-ray".into(),
            format: "PS3Z".into(),
            original_name: name,
            original_size,
            archive_size: Some(archive_size),
            sha256: Some(hex::encode(hash)),
            title_id: None,
            title: None,
            confidence: "stored archive metadata; run Verify".into(),
        },
        hash,
    ))
}
pub fn inspect_archive(path: &Path) -> Result<Ps3Info, String> {
    let mut f = File::open(path).map_err(fail)?;
    Ok(read_header(&mut f, path)?.0)
}
fn decode(path: &Path, output: Option<&Path>, cancel: &AtomicBool) -> Result<Ps3Info, String> {
    let mut f = File::open(path).map_err(fail)?;
    let (info, expected) = read_header(&mut f, path)?;
    let mut decoder = zstd::stream::read::Decoder::new(f).map_err(fail)?;
    let mut out = match output {
        Some(p) => Some(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(p)
                .map_err(fail)?,
        ),
        None => None,
    };
    let result = (|| {
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled".into());
            }
            let n = decoder.read(&mut buf).map_err(fail)?;
            if n == 0 {
                break;
            }
            count = count.checked_add(n as u64).ok_or("Decoded size overflow")?;
            if count > info.original_size {
                return Err("Archive expands beyond stored source size".into());
            }
            hash.update(&buf[..n]);
            if let Some(file) = out.as_mut() {
                file.write_all(&buf[..n]).map_err(fail)?;
            }
        }
        if count != info.original_size || hash.finalize().as_slice() != expected {
            return Err("Restored ISO differs from the stored source SHA-256 or size".into());
        }
        if let Some(file) = out.as_mut() {
            file.sync_all().map_err(fail)?;
        }
        Ok(info)
    })();
    drop(out);
    if result.is_err() {
        if let Some(p) = output {
            let _ = fs::remove_file(p);
        }
    }
    result
}
pub fn verify(path: &Path, cancel: &AtomicBool) -> Result<Ps3Report, String> {
    Ok(Ps3Report {
        info: decode(path, None, cancel)?,
        verified: true,
    })
}
pub fn extract(path: &Path, folder: &Path, cancel: &AtomicBool) -> Result<Ps3Report, String> {
    let info = inspect_archive(path)?;
    if !folder.is_dir() {
        return Err("Output folder does not exist".into());
    }
    let target = folder.join(&info.original_name);
    if target.exists() {
        return Err("Restored ISO already exists".into());
    }
    Ok(Ps3Report {
        info: decode(path, Some(&target), cancel)?,
        verified: true,
    })
}
pub fn compress(input: &Path, output: &Path, cancel: &AtomicBool) -> Result<Ps3Report, String> {
    let mut info = inspect(input)?;
    if output.exists() {
        return Err("Archive already exists".into());
    }
    let original_hash = digest(input, cancel)?;
    let name = info.original_name.as_bytes();
    if name.len() > 1024 {
        return Err("Source filename is too long".into());
    }
    let mut source = File::open(input).map_err(fail)?;
    let temp = output.with_extension(format!("ps3z.partial-{}", std::process::id()));
    let mut dest = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("Cannot create temporary archive: {e}"))?;
    let result = (|| {
        dest.write_all(MAGIC).map_err(fail)?;
        dest.write_all(&info.original_size.to_le_bytes())
            .map_err(fail)?;
        dest.write_all(&(name.len() as u16).to_le_bytes())
            .map_err(fail)?;
        dest.write_all(&original_hash).map_err(fail)?;
        dest.write_all(name).map_err(fail)?;
        let mut encoder = zstd::stream::write::Encoder::new(dest, 9).map_err(fail)?;
        let mut buf = [0u8; 64 * 1024];
        let mut count = 0u64;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("Cancelled".into());
            }
            let n = source.read(&mut buf).map_err(fail)?;
            if n == 0 {
                break;
            }
            encoder.write_all(&buf[..n]).map_err(fail)?;
            count += n as u64;
        }
        let file = encoder.finish().map_err(fail)?;
        file.sync_all().map_err(fail)?;
        if count != info.original_size || digest(input, cancel)? != original_hash {
            return Err("Source changed during compression".into());
        }
        decode(&temp, None, cancel).map_err(|e| format!("Temporary archive verification failed: {e}"))?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    if output.exists() {
        let _ = fs::remove_file(&temp);
        return Err("Archive already exists".into());
    }
    fs::rename(&temp, output).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("Cannot finalize archive: {e}")
    })?;
    info.archive_size = Some(fs::metadata(output).map_err(fail)?.len());
    info.sha256 = Some(hex::encode(original_hash));
    info.format = "PS3Z".into();
    info.confidence = "exact restoration verified".into();
    Ok(Ps3Report {
        info,
        verified: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cue_can_locate_one_iso_without_claiming_its_platform() {
        let root = std::env::temp_dir().join(format!("ps3-cue-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let iso = root.join("Install Disc.ISO");
        let cue = root.join("Install Disc.CUE");
        let mut data = vec![0u8; 128 * 2048];
        data[2048..2060].copy_from_slice(b"PlayStation3");
        data[32768..32774].copy_from_slice(b"\x01CD001");
        fs::write(&iso, data).unwrap();
        fs::write(&cue, b"FILE \"Install Disc.ISO\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
        let found = cue_iso_reference(&cue).unwrap().unwrap();
        assert_eq!(found, iso);
        assert_eq!(inspect(&found).unwrap().platform, "PlayStation 3");
        fs::write(&cue, b"FILE \"Install Disc.ISO\" BINARY\nFILE \"other.ISO\" BINARY\n").unwrap();
        assert!(cue_iso_reference(&cue).unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
    fn record(name: &[u8], sector: u32, size: u32) -> Vec<u8> {
        let mut row = vec![0u8; 33 + name.len()];
        row[0] = row.len() as u8;
        row[2..6].copy_from_slice(&sector.to_le_bytes());
        row[10..14].copy_from_slice(&size.to_le_bytes());
        row[32] = name.len() as u8;
        row[33..].copy_from_slice(name);
        row
    }
    #[test]
    fn reads_ps3_title_and_id_from_param_sfo() {
        let root = std::env::temp_dir().join(format!("ps3-sfo-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let iso = root.join("game.iso");
        let mut data = vec![0u8; 128 * 2048];
        data[2048..2060].copy_from_slice(b"PlayStation3");
        let pvd = 16 * 2048;
        data[pvd..pvd + 6].copy_from_slice(b"\x01CD001");
        let root_row = record(&[0], 20, 2048);
        data[pvd + 156..pvd + 156 + root_row.len()].copy_from_slice(&root_row);
        let game_row = record(b"PS3_GAME", 21, 2048);
        data[20 * 2048..20 * 2048 + game_row.len()].copy_from_slice(&game_row);
        let file_row = record(b"PARAM.SFO;1", 22, 92);
        data[21 * 2048..21 * 2048 + file_row.len()].copy_from_slice(&file_row);
        let mut sfo = vec![0u8; 92];
        sfo[..4].copy_from_slice(b"\0PSF");
        sfo[8..12].copy_from_slice(&52u32.to_le_bytes());
        sfo[12..16].copy_from_slice(&68u32.to_le_bytes());
        sfo[16..20].copy_from_slice(&2u32.to_le_bytes());
        sfo[22..24].copy_from_slice(&0x204u16.to_le_bytes());
        sfo[24..28].copy_from_slice(&13u32.to_le_bytes());
        sfo[28..32].copy_from_slice(&13u32.to_le_bytes());
        sfo[36..38].copy_from_slice(&6u16.to_le_bytes());
        sfo[38..40].copy_from_slice(&0x204u16.to_le_bytes());
        sfo[40..44].copy_from_slice(&10u32.to_le_bytes());
        sfo[44..48].copy_from_slice(&10u32.to_le_bytes());
        sfo[48..52].copy_from_slice(&13u32.to_le_bytes());
        sfo[52..67].copy_from_slice(b"TITLE\0TITLE_ID\0");
        sfo[68..81].copy_from_slice(b"Fixture Game\0");
        sfo[81..91].copy_from_slice(b"BLJM85001\0");
        data[22 * 2048..22 * 2048 + sfo.len()].copy_from_slice(&sfo);
        fs::write(&iso, &data).unwrap();
        let info = inspect(&iso).unwrap();
        assert_eq!(info.title.as_deref(), Some("Fixture Game"));
        assert_eq!(info.title_id.as_deref(), Some("BLJM85001"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn roundtrip_and_corruption() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.test-temp");
        fs::create_dir_all(&tests).unwrap();
        let root = tests.join(format!("ps3z-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let iso = root.join("disc.iso");
        let archive = root.join("disc.ps3z");
        let restored = root.join("restore");
        fs::create_dir_all(&restored).unwrap();
        let mut data = vec![0u8; 128 * 2048];
        data[2048..2060].copy_from_slice(b"PlayStation3");
        data[32768..32774].copy_from_slice(b"\x01CD001");
        data[120000..120010].copy_from_slice(b"test bytes");
        fs::write(&iso, &data).unwrap();
        let cancel = AtomicBool::new(false);
        assert!(inspect(&iso).is_ok());
        assert!(compress(&iso, &archive, &cancel).unwrap().verified);
        assert!(verify(&archive, &cancel).unwrap().verified);
        extract(&archive, &restored, &cancel).unwrap();
        assert_eq!(fs::read(restored.join("disc.iso")).unwrap(), data);
        let mut f = OpenOptions::new().write(true).open(&archive).unwrap();
        f.seek(SeekFrom::Start(20)).unwrap();
        f.write_all(&[0]).unwrap();
        drop(f);
        assert!(verify(&archive, &cancel).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn optional_real_iso_detection() {
        if let Ok(path) = std::env::var("PS3_TEST_ISO") {
            let source = inspect(Path::new(&path)).unwrap();
            assert_eq!(source.platform, "PlayStation 3");
            assert!(source.original_size > 2048);
            println!("PS3 metadata: title={:?}, title_id={:?}", source.title, source.title_id);
            if let Ok(expected) = std::env::var("PS3_EXPECT_TITLE_ID") {
                assert_eq!(source.title_id.as_deref(), Some(expected.as_str()));
            }
            if let Ok(cue) = std::env::var("PS3_TEST_CUE") {
                let linked = cue_iso_reference(Path::new(&cue)).unwrap().unwrap();
                assert_eq!(linked, Path::new(&path));
            }
        }
    }
    #[test]
    fn rejects_non_ps3_iso_and_existing_output() {
        let root = std::env::temp_dir().join(format!("ps3z-negative-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let iso = root.join("other.iso");
        let archive = root.join("other.ps3z");
        let mut data = vec![0u8; 128 * 2048];
        data[32768..32774].copy_from_slice(b"\x01CD001");
        fs::write(&iso, &data).unwrap();
        assert!(inspect(&iso).is_err());
        data[2048..2060].copy_from_slice(b"PlayStation3");
        fs::write(&iso, &data).unwrap();
        fs::write(&archive, b"keep me").unwrap();
        assert!(compress(&iso, &archive, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(&archive).unwrap(), b"keep me");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn optional_full_real_iso_roundtrip() {
        let (Ok(path), Some(_)) = (
            std::env::var("PS3_TEST_ISO"),
            std::env::var_os("PS3_FULL_TEST"),
        ) else {
            return;
        };
        let input = Path::new(&path);
        let workdir = std::env::var("PS3_TEST_WORKDIR")
            .expect("Set PS3_TEST_WORKDIR to a separate drive with at least 10 GiB free");
        let folder = Path::new(&workdir).join(format!("ps3z-full-{}", std::process::id()));
        fs::create_dir(&folder).unwrap();
        let archive = folder.join("archive.ps3z");
        let restored = folder.join("restored");
        fs::create_dir(&restored).unwrap();
        let cancel = AtomicBool::new(false);
        let result = (|| -> Result<(), String> {
            let compressed = compress(input, &archive, &cancel)?;
            assert!(compressed.verified);
            verify(&archive, &cancel)?;
            let extracted = extract(&archive, &restored, &cancel)?;
            assert!(extracted.verified);
            assert_eq!(
                digest(input, &cancel)?,
                digest(&restored.join(extracted.info.original_name), &cancel)?
            );
            println!(
                "PS3 ISO round trip passed: original={} archive={}",
                compressed.info.original_size,
                compressed.info.archive_size.unwrap()
            );
            Ok(())
        })();
        if result.is_ok() {
            fs::remove_dir_all(&folder).unwrap();
        }
        result.unwrap();
    }
}
