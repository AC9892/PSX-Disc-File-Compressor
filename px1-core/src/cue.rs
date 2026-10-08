use std::{fs::File, io::Read, path::{Path, PathBuf}};

const MAX_CUE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CueSource {
    pub cue_path: PathBuf, pub bin_path: PathBuf, pub cue_bytes: Vec<u8>,
    pub track_count: u32, pub data_track_count: u32, pub data_track_start_sector: u64,
}

fn cue_number(value: &str) -> Option<u32> {
    if value.len() != 2 || !value.bytes().all(|b| b.is_ascii_digit()) { return None; }
    value.parse().ok()
}

fn cue_time(value: &str) -> Option<u64> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 3 || !parts.iter().all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit())) { return None; }
    let minutes=parts[0].parse::<u64>().ok()?;
    let seconds=parts[1].parse::<u64>().ok()?;
    let frames=parts[2].parse::<u64>().ok()?;
    if seconds >= 60 || frames >= 75 { return None; }
    Some((minutes*60+seconds)*75+frames)
}

pub fn parse(path: &Path) -> Result<CueSource, String> {
    let file = File::open(path).map_err(|e| format!("Cannot read CUE: {e}"))?;
    if file.metadata().map_err(|e| format!("Cannot stat CUE: {e}"))?.len() > MAX_CUE_BYTES {
        return Err("CUE is too large".into());
    }
    let mut cue_bytes = Vec::new();
    file.take(MAX_CUE_BYTES + 1).read_to_end(&mut cue_bytes).map_err(|e| format!("Cannot read CUE: {e}"))?;
    if cue_bytes.len() as u64 > MAX_CUE_BYTES { return Err("CUE is too large".into()); }
    let raw_text = cue_bytes.strip_prefix(&[0xef,0xbb,0xbf]).unwrap_or(&cue_bytes);
    for raw_line in raw_text.split(|byte| *byte == b'\n') {
        let first = raw_line.iter().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(raw_line.len());
        let raw_line = &raw_line[first..];
        if raw_line.len() >= 4 && raw_line[..4].eq_ignore_ascii_case(b"FILE")
            && raw_line.get(4).map_or(true, u8::is_ascii_whitespace)
            && std::str::from_utf8(raw_line).is_err() {
            return Err("Unsupported CUE filename encoding: FILE directive is not UTF-8".into());
        }
    }
    let text = String::from_utf8_lossy(&cue_bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut files = Vec::new();
    let mut tracks = 0;
    let mut data_tracks = 0;
    let mut index_for_track = true;
    let mut last_track_number = 0;
    let mut current_is_data = false;
    let mut data_track_start_sector = None;
    let mut max_index_sector = 0;
    for line in text.lines() {
        let line = line.trim_start();
        let mut parts = line.splitn(2, char::is_whitespace);
        let keyword = parts.next().unwrap_or("");
        if keyword.eq_ignore_ascii_case("FILE") {
            let rest = parts.next().unwrap_or("").trim();
            let (name, kind) = if let Some(after) = rest.strip_prefix('"') {
                let end = after.find('"').ok_or("Invalid CUE FILE quotation")?;
                (&after[..end], after[end+1..].trim())
            } else {
                let pos = rest.rfind(char::is_whitespace).ok_or("Invalid CUE FILE directive")?;
                (rest[..pos].trim(), rest[pos..].trim())
            };
            if !kind.eq_ignore_ascii_case("BINARY") { return Err("Unsupported CUE layout: only BINARY image files are supported".into()); }
            let label = Path::new(name);
            if name.is_empty() || label.components().count() != 1 || label.file_name().is_none() {
                return Err("Unsupported CUE layout: FILE must name a file beside the CUE".into());
            }
            files.push(name.to_owned());
        } else if keyword.eq_ignore_ascii_case("TRACK") {
            if files.is_empty() || !index_for_track { return Err("Invalid CUE: each track needs a FILE and INDEX 01".into()); }
            let rest = parts.next().unwrap_or("").trim();
            let mut fields = rest.split_whitespace();
            let number = cue_number(fields.next().unwrap_or("")).ok_or("Invalid CUE track number")?;
            if number != last_track_number + 1 || fields.clone().count() != 1 {
                return Err("Invalid CUE track sequence or TRACK directive".into());
            }
            let mode = fields.next().unwrap_or("");
            if !matches!(mode.to_ascii_uppercase().as_str(), "MODE1/2352" | "MODE2/2352" | "AUDIO") {
                return Err(format!("Unsupported CUE track mode: {mode}"));
            }
            tracks += 1;
            last_track_number = number;
            index_for_track = false;
            current_is_data = !mode.eq_ignore_ascii_case("AUDIO");
            if current_is_data { data_tracks += 1; }
        } else if keyword.eq_ignore_ascii_case("INDEX") {
            if tracks == 0 { return Err("Invalid CUE: INDEX precedes TRACK".into()); }
            let mut fields = parts.next().unwrap_or("").split_whitespace();
            let number = cue_number(fields.next().unwrap_or("")).ok_or("Invalid CUE INDEX number")?;
            let time = fields.next().unwrap_or("");
            let sector = cue_time(time).ok_or("Invalid CUE INDEX timestamp")?;
            if fields.next().is_some() { return Err("Invalid CUE INDEX timestamp".into()); }
            max_index_sector = max_index_sector.max(sector);
            if number == 1 {
                if index_for_track { return Err("Invalid CUE: duplicate INDEX 01".into()); }
                index_for_track = true;
                if current_is_data && data_track_start_sector.is_none() { data_track_start_sector = Some(sector); }
            }
        }
    }
    if files.len() != 1 || tracks == 0 || data_tracks == 0 || !index_for_track {
        return Err("Unsupported CUE layout: PX1 v0.1 requires one referenced BIN, a data track, and INDEX 01 for each track".into());
    }
    let bin_path = path.parent().unwrap_or(Path::new(".")).join(&files[0]);
    if !bin_path.is_file() { return Err(format!("Referenced BIN is missing: {}", bin_path.display())); }
    let sectors = bin_path.metadata().map_err(|e| format!("Cannot stat BIN: {e}"))?.len() / 2352;
    if max_index_sector >= sectors { return Err("Invalid CUE: INDEX points beyond the referenced BIN".into()); }
    Ok(CueSource { cue_path: path.to_path_buf(), bin_path, cue_bytes, track_count: tracks, data_track_count: data_tracks, data_track_start_sector: data_track_start_sector.ok_or("Invalid CUE: data track has no INDEX 01")? })
}
