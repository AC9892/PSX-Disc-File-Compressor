use serde::{Deserialize, Serialize};
use std::{fs::File, io::{Read, Seek, SeekFrom}, path::Path};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscMetadata {
    pub platform: Option<String>, pub media: Option<String>, pub serial: Option<String>,
    pub region: Option<String>, pub volume_id: Option<String>, pub boot_executable: Option<String>,
    pub title: Option<String>, pub disc_number: Option<u32>,
}

fn sector(file: &mut File, base: u64, n: u64) -> Option<[u8; 2048]> {
    let mut raw = [0u8; 2352];
    file.seek(SeekFrom::Start(base.checked_add(n)?.checked_mul(2352)?)).ok()?;
    file.read_exact(&mut raw).ok()?;
    if raw[0] != 0 || raw[1..11] != [0xff; 10] || raw[11] != 0 { return None; }
    let offset = match raw[15] { 1 => 16, 2 => 24, _ => return None };
    let mut result = [0u8; 2048];
    result.copy_from_slice(&raw[offset..offset+2048]);
    Some(result)
}

fn iso_file(file: &mut File, base: u64, root_sector: u64, root_size: usize, wanted: &str) -> Option<Vec<u8>> {
    if root_size > 1024 * 1024 { return None; }
    let mut directory = Vec::with_capacity(root_size);
    for n in 0..root_size.div_ceil(2048) { directory.extend_from_slice(&sector(file, base, root_sector + n as u64)?); }
    let mut at = 0;
    while at < root_size {
        let len = directory[at] as usize;
        if len == 0 { at = (at / 2048 + 1) * 2048; continue; }
        if at + len > directory.len() || len < 34 { return None; }
        let entry = &directory[at..at+len];
        let name_len = entry[32] as usize;
        if 33 + name_len <= len {
            let name = String::from_utf8_lossy(&entry[33..33+name_len]);
            if name.split(';').next().unwrap_or("").eq_ignore_ascii_case(wanted) {
                let extent = u32::from_le_bytes(entry[2..6].try_into().ok()?) as u64;
                let size = u32::from_le_bytes(entry[10..14].try_into().ok()?) as usize;
                if size > 64 * 1024 { return None; }
                let mut data = Vec::with_capacity(size);
                for n in 0..size.div_ceil(2048) { data.extend_from_slice(&sector(file, base, extent + n as u64)?); }
                data.truncate(size);
                return Some(data);
            }
        }
        at += len;
    }
    None
}

fn serial_from_executable(exe: &str) -> Option<(String, &'static str)> {
    let normalized = exe.replace('_', "-").replace('.', "").to_ascii_uppercase();
    let bytes = normalized.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || !bytes[5..].iter().all(u8::is_ascii_digit) { return None; }
    let region = match &normalized[..4] {
        "SLUS" | "SCUS" => "USA",
        "SLES" | "SCES" | "SCED" | "SLED" => "Europe",
        "SLPS" | "SCPS" | "SLPM" | "SCZS" => "Japan",
        "SLKA" => "Korea",
        _ => return None,
    };
    Some((normalized, region))
}

pub fn detect(path: &Path, data_track_start_sector: u64) -> DiscMetadata {
    let mut out = DiscMetadata::default();
    let Ok(mut file) = File::open(path) else { return out };
    let Some(pvd) = sector(&mut file, data_track_start_sector, 16) else { return out };
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" || pvd[6] != 1 { return out; }
    out.media = Some("CD-ROM".into());
    let vol = String::from_utf8_lossy(&pvd[40..72]).trim_matches([' ', '\0']).to_string();
    if !vol.is_empty() { out.volume_id = Some(vol); }
    let root = &pvd[156..];
    if root.len() < 34 || root[0] < 34 { return out; }
    let extent = u32::from_le_bytes(root[2..6].try_into().unwrap()) as u64;
    let size = u32::from_le_bytes(root[10..14].try_into().unwrap()) as usize;
    let Some(cnf) = iso_file(&mut file, data_track_start_sector, extent, size, "SYSTEM.CNF") else { return out };
    let cnf = String::from_utf8_lossy(&cnf);
    for line in cnf.lines() {
        let Some((key, val)) = line.split_once('=') else { continue };
        if !key.trim().eq_ignore_ascii_case("BOOT") { continue; }
        let value = val.trim().trim_matches('"');
        let exe = value.rsplit(['\\', '/']).next().unwrap_or(value).split(';').next().unwrap_or("").trim();
        if exe.is_empty() { continue; }
        out.platform = Some("PlayStation".into());
        out.boot_executable = Some(value.to_string());
        if let Some((serial, region)) = serial_from_executable(exe) {
            out.region = Some(region.into());
            out.serial = Some(serial);
        }
        break;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::serial_from_executable;

    #[test]
    fn accepts_only_exact_boot_product_codes() {
        assert_eq!(serial_from_executable("SLUS_008.92"), Some(("SLUS-00892".into(), "USA")));
        assert_eq!(serial_from_executable("SLES_012.34"), Some(("SLES-01234".into(), "Europe")));
        assert_eq!(serial_from_executable("SLUS_GAME.EXE"), None);
        assert_eq!(serial_from_executable("SLUS_008.92.EXE"), None);
    }
}
