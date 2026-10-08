use super::{compression::{self, ChunkCodec}, cue, metadata::{self, DiscMetadata}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, path::Path, sync::atomic::{AtomicBool, Ordering}, time::Instant};

const MAGIC: &[u8; 4] = b"PX1\0";
const HEADER_SIZE: u64 = 96;
const ENTRY_SIZE: u64 = 56;
const MIXED_ENTRY_SIZE: u64 = 60;
const CHUNK_SIZE: usize = 4 * 1024 * 1024;
const FOOTER_SIZE: u64 = 32;
const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_CUE: u64 = 8 * 1024 * 1024;
const RAW_SECTOR_SIZE: u64 = 2352;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String, pub size: u64, pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32, #[serde(default)] pub format_minor: u16, pub created_utc: String,
    pub bin: FileInfo, pub cue: FileInfo, pub disc: DiscMetadata,
    pub compression: String, pub zstd_level: i32, pub chunk_size: u32, pub chunk_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscInfo {
    pub cue_path: String, pub bin_path: String, pub bin_size: u64, pub cue_size: u64,
    pub referenced_bin_name: String, pub track_count: u32, pub data_track_count: u32, pub disc: DiscMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress { pub stage: String, pub done: u64, pub total: u64, pub bytes_written: u64, pub elapsed_seconds: u64 }

#[derive(Debug, Clone, Serialize)]
pub struct Report { pub manifest: Manifest, pub px1_size: u64, pub container_sha256: String, pub chunks_verified: u64, pub container_verified: bool, pub bin_verified: bool, pub cue_verified: bool }

#[derive(Debug, Clone)]
struct Header { minor: u16, compression_id: u32, manifest_off: u64, manifest_len: u64, cue_off: u64, cue_len: u64, table_off: u64, table_len: u64, data_off: u64, chunks: u64, footer_off: u64 }

impl Header { fn entry_size(&self) -> u64 { if self.minor == 0 { ENTRY_SIZE } else { MIXED_ENTRY_SIZE } } }

#[derive(Debug, Clone)]
struct Entry { source_off: u64, compressed_off: u64, source_len: u32, compressed_len: u32, hash: [u8; 32], codec: ChunkCodec }

fn err<E: std::fmt::Display>(context: &str, e: E) -> String { format!("{context}: {e}") }
fn checked_cancel(cancel: &AtomicBool) -> Result<(), String> { if cancel.load(Ordering::Relaxed) { Err("Operation cancelled".into()) } else { Ok(()) } }
fn sha_bytes(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }
fn safe_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.ends_with([' ', '.'])
        || name.chars().any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || Path::new(name).file_name().is_none() { return false; }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") { return false; }
    if stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0' { return false; }
    true
}
fn file_name(path: &Path) -> Result<String, String> { let name = path.file_name().ok_or("File has no name")?.to_string_lossy().to_string(); if safe_name(&name) { Ok(name) } else { Err("Unsafe source filename".into()) } }
fn validate_bin_size(size: u64) -> Result<(), String> {
    if size == 0 || size % RAW_SECTOR_SIZE != 0 {
        Err("Unsupported BIN: expected a nonempty 2352-byte-sector image".into())
    } else { Ok(()) }
}
fn put_u64(buf: &mut [u8], at: usize, value: u64) { buf[at..at+8].copy_from_slice(&value.to_le_bytes()); }
fn get_u64(buf: &[u8], at: usize) -> u64 { u64::from_le_bytes(buf[at..at+8].try_into().unwrap()) }

fn header_bytes(h: &Header) -> [u8; HEADER_SIZE as usize] {
    let mut b = [0u8; HEADER_SIZE as usize];
    b[..4].copy_from_slice(MAGIC);
    b[4..6].copy_from_slice(&1u16.to_le_bytes());
    b[6..8].copy_from_slice(&h.minor.to_le_bytes());
    b[8..12].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    for (i, value) in [h.manifest_off,h.manifest_len,h.cue_off,h.cue_len,h.table_off,h.table_len,h.data_off,h.chunks,h.footer_off].iter().enumerate() { put_u64(&mut b,16+i*8,*value); }
    b[88..92].copy_from_slice(&h.compression_id.to_le_bytes());
    b[92..96].copy_from_slice(&(CHUNK_SIZE as u32).to_le_bytes());
    b
}

fn read_header(file: &mut File) -> Result<(Header, u64), String> {
    let len = file.metadata().map_err(|e| err("Cannot stat PX1",e))?.len();
    if len < HEADER_SIZE + FOOTER_SIZE { return Err("Truncated PX1 header or footer".into()); }
    let mut b = [0u8; HEADER_SIZE as usize];
    file.seek(SeekFrom::Start(0)).map_err(|e| err("Cannot seek PX1",e))?;
    file.read_exact(&mut b).map_err(|e| err("Cannot read PX1 header",e))?;
    if &b[..4] != MAGIC { return Err("Invalid PX1 magic".into()); }
    if u16::from_le_bytes([b[4],b[5]]) != 1 { return Err("Unsupported PX1 major version".into()); }
    let minor = u16::from_le_bytes([b[6],b[7]]);
    if minor > 1 { return Err("Unsupported PX1 minor version".into()); }
    if u32::from_le_bytes(b[8..12].try_into().unwrap()) != HEADER_SIZE as u32 || b[12..16] != [0;4] { return Err("Invalid PX1 header size or flags".into()); }
    let compression_id = u32::from_le_bytes(b[88..92].try_into().unwrap());
    if compression_id != if minor == 0 { 1 } else { 2 } || u32::from_le_bytes(b[92..96].try_into().unwrap()) != CHUNK_SIZE as u32 { return Err("Unsupported PX1 compression settings".into()); }
    let h = Header { minor, compression_id, manifest_off:get_u64(&b,16), manifest_len:get_u64(&b,24), cue_off:get_u64(&b,32), cue_len:get_u64(&b,40), table_off:get_u64(&b,48), table_len:get_u64(&b,56), data_off:get_u64(&b,64), chunks:get_u64(&b,72), footer_off:get_u64(&b,80) };
    if h.cue_len > MAX_CUE { return Err("CUE is too large".into()); }
    if h.manifest_off != HEADER_SIZE || h.manifest_len == 0 || h.manifest_len > MAX_MANIFEST || h.cue_off != h.manifest_off.checked_add(h.manifest_len).ok_or("PX1 offset overflow")? || h.table_off != h.cue_off.checked_add(h.cue_len).ok_or("PX1 offset overflow")? || h.table_len != h.chunks.checked_mul(h.entry_size()).ok_or("PX1 chunk table overflow")? || h.data_off != h.table_off.checked_add(h.table_len).ok_or("PX1 offset overflow")? || h.footer_off < h.data_off || h.footer_off.checked_add(FOOTER_SIZE) != Some(len) { return Err("Invalid PX1 section offsets or lengths".into()); }
    Ok((h,len))
}

fn write_entry(file: &mut File, entry: &Entry, mixed: bool) -> Result<(), String> {
    file.write_all(&entry.source_off.to_le_bytes()).map_err(|e| err("Write chunk table",e))?;
    file.write_all(&entry.compressed_off.to_le_bytes()).map_err(|e| err("Write chunk table",e))?;
    file.write_all(&entry.source_len.to_le_bytes()).map_err(|e| err("Write chunk table",e))?;
    file.write_all(&entry.compressed_len.to_le_bytes()).map_err(|e| err("Write chunk table",e))?;
    file.write_all(&entry.hash).map_err(|e| err("Write chunk table",e))?;
    if mixed { file.write_all(&(entry.codec as u32).to_le_bytes()).map_err(|e| err("Write chunk codec",e))?; }
    Ok(())
}

fn read_entries(file: &mut File, h: &Header, bin_size: u64) -> Result<Vec<Entry>, String> {
    let expected = bin_size.div_ceil(CHUNK_SIZE as u64);
    if h.chunks != expected { return Err("Chunk count does not match BIN size".into()); }
    let mut entries = Vec::with_capacity(expected.try_into().map_err(|_| "Too many chunks")?);
    file.seek(SeekFrom::Start(h.table_off)).map_err(|e| err("Seek chunk table",e))?;
    let mut source_off = 0u64;
    let mut compressed_off = h.data_off;
    for _ in 0..h.chunks {
        let mut b = [0u8; MIXED_ENTRY_SIZE as usize];
        file.read_exact(&mut b[..h.entry_size() as usize]).map_err(|e| err("Read chunk table",e))?;
        let codec = if h.minor == 0 { ChunkCodec::Zstd } else {
            ChunkCodec::from_id(u32::from_le_bytes(b[56..60].try_into().unwrap()))?
        };
        let entry = Entry { source_off:get_u64(&b,0), compressed_off:get_u64(&b,8), source_len:u32::from_le_bytes(b[16..20].try_into().unwrap()), compressed_len:u32::from_le_bytes(b[20..24].try_into().unwrap()), hash:b[24..56].try_into().unwrap(), codec };
        let remaining = bin_size - source_off;
        if entry.source_off != source_off || entry.source_len as u64 != remaining.min(CHUNK_SIZE as u64) || entry.compressed_off != compressed_off || entry.compressed_len == 0 || entry.compressed_len as u64 > CHUNK_SIZE as u64 + 1024 * 1024 { return Err("Invalid chunk table entry".into()); }
        source_off += entry.source_len as u64;
        compressed_off = compressed_off.checked_add(entry.compressed_len as u64).ok_or("Chunk offset overflow")?;
        if compressed_off > h.footer_off { return Err("Chunk data exceeds PX1 bounds".into()); }
        entries.push(entry);
    }
    if compressed_off != h.footer_off { return Err("Chunk table does not cover data section".into()); }
    Ok(entries)
}

fn read_manifest(file: &mut File, h: &Header) -> Result<Manifest, String> {
    file.seek(SeekFrom::Start(h.manifest_off)).map_err(|e| err("Seek manifest",e))?;
    let mut bytes = vec![0u8; h.manifest_len as usize];
    file.read_exact(&mut bytes).map_err(|e| err("Read manifest",e))?;
    let m: Manifest = serde_json::from_slice(&bytes).map_err(|e| err("Invalid manifest",e))?;
    validate_bin_size(m.bin.size)?;
    let expected_compression = if h.minor == 0 { "zstd" } else { "mixed" };
    if m.format_version != 1 || m.format_minor != h.minor || m.compression != expected_compression || m.chunk_size != CHUNK_SIZE as u32 || m.chunk_count != h.chunks || m.cue.size != h.cue_len || !safe_name(&m.bin.name) || !safe_name(&m.cue.name) || m.bin.name.eq_ignore_ascii_case(&m.cue.name) || m.bin.sha256.len()!=64 || m.cue.sha256.len()!=64 || hex::decode(&m.bin.sha256).map_or(true, |x|x.len()!=32) || hex::decode(&m.cue.sha256).map_or(true, |x|x.len()!=32) { return Err("Invalid manifest fields".into()); }
    Ok(m)
}

fn hash_file(path: &Path, total: u64, cancel: &AtomicBool, progress: &impl Fn(Progress), start: Instant) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| err("Open BIN",e))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; CHUNK_SIZE];
    let mut done = 0;
    loop {
        checked_cancel(cancel)?;
        let n = f.read(&mut buf).map_err(|e| err("Read BIN",e))?;
        if n==0 { break; }
        hash.update(&buf[..n]);
        done += n as u64;
        progress(Progress{stage:"Hashing source".into(),done,total,bytes_written:0,elapsed_seconds:start.elapsed().as_secs()});
    }
    Ok(hex::encode(hash.finalize()))
}

pub fn source_info(path: &Path) -> Result<DiscInfo, String> {
    source_info_with_bin(path, None)
}

pub fn source_info_with_bin(path: &Path, selected_bin: Option<&Path>) -> Result<DiscInfo, String> {
    let src = cue::parse_with_bin(path, selected_bin)?;
    let bin_size = src.bin_path.metadata().map_err(|e| err("Cannot stat BIN",e))?.len();
    validate_bin_size(bin_size)?;
    if !safe_name(&src.referenced_bin_name) { return Err("Unsafe BIN filename in CUE".into()); }
    Ok(DiscInfo { cue_path:src.cue_path.to_string_lossy().into(), bin_path:src.bin_path.to_string_lossy().into(), bin_size, cue_size:src.cue_bytes.len() as u64, referenced_bin_name:src.referenced_bin_name, track_count:src.track_count, data_track_count:src.data_track_count, disc:metadata::detect(&src.bin_path,src.data_track_start_sector) })
}

pub fn compress(path: &Path, output: &Path, level: i32, title: Option<String>, disc_number: Option<u32>, cancel: &AtomicBool, progress: impl Fn(Progress)) -> Result<Report, String> {
    compress_with_bin(path, None, output, level, title, disc_number, cancel, progress)
}

pub fn compress_with_bin(path: &Path, selected_bin: Option<&Path>, output: &Path, level: i32, title: Option<String>, disc_number: Option<u32>, cancel: &AtomicBool, progress: impl Fn(Progress)) -> Result<Report, String> {
    compress_with_bin_strategy(path, selected_bin, output, level, false, title, disc_number, cancel, progress)
}

pub fn compress_with_bin_strategy(path: &Path, selected_bin: Option<&Path>, output: &Path, level: i32, best_size: bool, title: Option<String>, disc_number: Option<u32>, cancel: &AtomicBool, progress: impl Fn(Progress)) -> Result<Report, String> {
    if !(-7..=22).contains(&level) { return Err("Invalid zstd level".into()); }
    let src = cue::parse_with_bin(path, selected_bin)?;
    let bin_size = src.bin_path.metadata().map_err(|e| err("Cannot stat BIN",e))?.len();
    validate_bin_size(bin_size)?;
    if !safe_name(&src.referenced_bin_name) { return Err("Unsafe BIN filename in CUE".into()); }
    let cue_name = file_name(&src.cue_path)?;
    if src.referenced_bin_name.eq_ignore_ascii_case(&cue_name) { return Err("CUE and BIN output filenames must differ".into()); }
    let source_bin_name = file_name(&src.bin_path)?;
    let source_name = (source_bin_name != src.referenced_bin_name).then_some(source_bin_name);
    let cue_size = src.cue_bytes.len() as u64;
    let out_name = output.file_name().ok_or("Invalid output filename")?.to_string_lossy();
    if !out_name.to_ascii_lowercase().ends_with(".px1") { return Err("Output filename must end in .px1".into()); }
    if output.exists() { return Err("Output PX1 already exists".into()); }
    if output == src.bin_path || output == src.cue_path { return Err("Output cannot be a source file".into()); }
    let temp = output.with_file_name(format!("{out_name}.tmp"));
    if temp.exists() { return Err(format!("Temporary output already exists: {}",temp.display())); }
    let start = Instant::now();
    progress(Progress{stage:"Hashing source".into(),done:0,total:bin_size,bytes_written:0,elapsed_seconds:0});
    let bin_hash = hash_file(&src.bin_path,bin_size,cancel,&progress,start)?;
    let mut disc = metadata::detect(&src.bin_path,src.data_track_start_sector);
    disc.title = title.filter(|x|!x.trim().is_empty());
    disc.disc_number = disc_number.filter(|x|*x>0);
    let chunks = bin_size.div_ceil(CHUNK_SIZE as u64);
    let minor = if best_size { 1 } else { 0 };
    let manifest = Manifest { format_version:1, format_minor:minor, created_utc:chrono::Utc::now().to_rfc3339(), bin:FileInfo{name:src.referenced_bin_name,size:bin_size,sha256:bin_hash,source_name}, cue:FileInfo{name:cue_name,size:cue_size,sha256:sha_bytes(&src.cue_bytes),source_name:None}, disc, compression:if best_size { "mixed" } else { "zstd" }.into(),zstd_level:level,chunk_size:CHUNK_SIZE as u32,chunk_count:chunks };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|e| err("Serialize manifest",e))?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST { return Err("Manifest is too large".into()); }
    let table_len = chunks.checked_mul(if best_size { MIXED_ENTRY_SIZE } else { ENTRY_SIZE }).ok_or("Chunk table too large")?;
    let header = Header { minor, compression_id:if best_size { 2 } else { 1 }, manifest_off:HEADER_SIZE, manifest_len:manifest_bytes.len() as u64, cue_off:HEADER_SIZE+manifest_bytes.len() as u64, cue_len:cue_size, table_off:HEADER_SIZE+manifest_bytes.len() as u64+cue_size, table_len, data_off:HEADER_SIZE+manifest_bytes.len() as u64+cue_size+table_len, chunks, footer_off:0 };
    let result = (|| -> Result<Report,String> {
        let mut out = OpenOptions::new().write(true).read(true).create_new(true).open(&temp).map_err(|e| err("Create temporary PX1",e))?;
        out.write_all(&[0;HEADER_SIZE as usize]).map_err(|e|err("Write header placeholder",e))?;
        out.write_all(&manifest_bytes).map_err(|e|err("Write manifest",e))?;
        out.write_all(&src.cue_bytes).map_err(|e|err("Write CUE",e))?;
        out.set_len(header.data_off).map_err(|e|err("Reserve chunk table",e))?;
        out.seek(SeekFrom::Start(header.data_off)).map_err(|e|err("Seek data",e))?;
        let mut input = File::open(&src.bin_path).map_err(|e|err("Open BIN",e))?;
        let mut raw = vec![0u8;CHUNK_SIZE];
        let mut entries = Vec::with_capacity(chunks.try_into().map_err(|_|"Too many chunks")?);
        for n in 0..chunks {
            checked_cancel(cancel)?;
            let source_off = n*CHUNK_SIZE as u64;
            let size = (bin_size-source_off).min(CHUNK_SIZE as u64) as usize;
            input.read_exact(&mut raw[..size]).map_err(|e|err("Read BIN chunk",e))?;
            let compressed = if best_size { compression::encode_best_chunk(&raw[..size],level,cancel)? } else { compression::encode_chunk(&raw[..size],level)? };
            let compressed_off = out.stream_position().map_err(|e|err("Get PX1 position",e))?;
            out.write_all(&compressed.data).map_err(|e|err("Write BIN chunk",e))?;
            entries.push(Entry{source_off,compressed_off,source_len:size as u32,compressed_len:compressed.data.len().try_into().map_err(|_|"Compressed chunk too large")?,hash:compressed.source_sha256,codec:compressed.codec});
            progress(Progress{stage:"Compressing".into(),done:n+1,total:chunks,bytes_written:out.stream_position().unwrap_or(0),elapsed_seconds:start.elapsed().as_secs()});
        }
        if input.read(&mut [0u8;1]).map_err(|e|err("Check BIN size",e))? != 0 { return Err("BIN changed while compressing".into()); }
        let footer_off = out.stream_position().map_err(|e|err("Get PX1 size",e))?;
        out.seek(SeekFrom::Start(header.table_off)).map_err(|e|err("Seek chunk table",e))?;
        for entry in &entries { write_entry(&mut out,entry,best_size)?; }
        let final_header = Header{footer_off,..header};
        out.seek(SeekFrom::Start(0)).map_err(|e|err("Seek header",e))?;
        out.write_all(&header_bytes(&final_header)).map_err(|e|err("Write header",e))?;
        out.flush().map_err(|e|err("Flush PX1",e))?;
        out.seek(SeekFrom::Start(0)).map_err(|e|err("Seek PX1 for hash",e))?;
        let mut hash = Sha256::new(); let mut buf = vec![0u8;CHUNK_SIZE]; let mut left = footer_off;
        while left>0 {
            checked_cancel(cancel)?;
            let n = (left as usize).min(buf.len());
            out.read_exact(&mut buf[..n]).map_err(|e|err("Read PX1 for hash",e))?;
            hash.update(&buf[..n]); left-=n as u64;
            progress(Progress{stage:"Sealing container".into(),done:footer_off-left,total:footer_off,bytes_written:footer_off,elapsed_seconds:start.elapsed().as_secs()});
        }
        out.seek(SeekFrom::Start(footer_off)).map_err(|e|err("Seek footer",e))?;
        out.write_all(&hash.finalize()).map_err(|e|err("Write footer",e))?;
        out.sync_all().map_err(|e|err("Sync PX1",e))?;
        drop(out);
        progress(Progress{stage:"Verifying".into(),done:0,total:chunks,bytes_written:footer_off+FOOTER_SIZE,elapsed_seconds:start.elapsed().as_secs()});
        let report = verify(&temp,cancel,|p|progress(p))?;
        checked_cancel(cancel)?;
        fs::rename(&temp,output).map_err(|e|err("Finalize PX1",e))?;
        Ok(report)
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result
}

pub fn inspect(path: &Path) -> Result<(Manifest,u64,String),String> {
    let mut file = File::open(path).map_err(|e|err("Open PX1",e))?;
    let (h,len)=read_header(&mut file)?;
    let manifest=read_manifest(&mut file,&h)?;
    read_entries(&mut file,&h,manifest.bin.size)?;
    file.seek(SeekFrom::Start(h.footer_off)).map_err(|e|err("Seek container hash",e))?;
    let mut footer=[0u8;32];
    file.read_exact(&mut footer).map_err(|e|err("Read container hash",e))?;
    Ok((manifest,len,hex::encode(footer)))
}

pub fn verify(path: &Path, cancel: &AtomicBool, progress: impl Fn(Progress)) -> Result<Report,String> {
    let start=Instant::now();
    let mut file=File::open(path).map_err(|e|err("Open PX1",e))?;
    let (h,len)=read_header(&mut file)?;
    let manifest=read_manifest(&mut file,&h)?;
    let entries=read_entries(&mut file,&h,manifest.bin.size)?;
    file.seek(SeekFrom::Start(h.cue_off)).map_err(|e|err("Seek CUE",e))?;
    let mut cue=vec![0u8;h.cue_len as usize];
    file.read_exact(&mut cue).map_err(|e|err("Read CUE",e))?;
    if sha_bytes(&cue)!=manifest.cue.sha256 { return Err("CUE SHA-256 mismatch".into()); }
    let mut bin_hash=Sha256::new();
    for (n,entry) in entries.iter().enumerate() {
        checked_cancel(cancel)?;
        file.seek(SeekFrom::Start(entry.compressed_off)).map_err(|e|err("Seek chunk",e))?;
        let mut compressed=vec![0u8;entry.compressed_len as usize];
        file.read_exact(&mut compressed).map_err(|e|err("Read chunk",e))?;
        let raw=compression::decode_chunk_codec(&compressed,entry.source_len as usize,&entry.hash,entry.codec).map_err(|e|format!("Chunk {} {e}",n+1))?;
        bin_hash.update(&raw);
        progress(Progress{stage:"Verifying".into(),done:n as u64+1,total:h.chunks,bytes_written:len,elapsed_seconds:start.elapsed().as_secs()});
    }
    if hex::encode(bin_hash.finalize())!=manifest.bin.sha256 { return Err("BIN SHA-256 mismatch".into()); }
    file.seek(SeekFrom::Start(0)).map_err(|e|err("Seek container",e))?;
    let mut hash=Sha256::new(); let mut buf=vec![0u8;CHUNK_SIZE]; let mut left=h.footer_off;
    progress(Progress{stage:"Checking container".into(),done:0,total:h.footer_off,bytes_written:len,elapsed_seconds:start.elapsed().as_secs()});
    while left>0 {
        checked_cancel(cancel)?;
        let n=(left as usize).min(buf.len());
        file.read_exact(&mut buf[..n]).map_err(|e|err("Read container",e))?;
        hash.update(&buf[..n]); left-=n as u64;
        progress(Progress{stage:"Checking container".into(),done:h.footer_off-left,total:h.footer_off,bytes_written:len,elapsed_seconds:start.elapsed().as_secs()});
    }
    let mut footer=[0u8;32]; file.read_exact(&mut footer).map_err(|e|err("Read footer",e))?;
    if hash.finalize().as_slice()!=footer { return Err("Container SHA-256 mismatch".into()); }
    Ok(Report{manifest,px1_size:len,container_sha256:hex::encode(footer),chunks_verified:h.chunks,container_verified:true,bin_verified:true,cue_verified:true})
}

pub fn extract(path: &Path, dest: &Path, cancel: &AtomicBool, progress: impl Fn(Progress)) -> Result<Report,String> {
    let report=verify(path,cancel,|p|progress(p))?;
    let start=Instant::now();
    if !dest.is_dir() { return Err("Output directory does not exist".into()); }
    let bin_path=dest.join(&report.manifest.bin.name);
    let cue_path=dest.join(&report.manifest.cue.name);
    if bin_path.exists() || cue_path.exists() { return Err("An extracted output file already exists".into()); }
    let bin_temp=dest.join(format!("{}.px1.tmp",report.manifest.bin.name));
    let cue_temp=dest.join(format!("{}.px1.tmp",report.manifest.cue.name));
    if bin_temp.exists() || cue_temp.exists() { return Err("Temporary extraction file already exists".into()); }
    let result=(|| -> Result<(),String> {
        let mut input=File::open(path).map_err(|e|err("Open PX1",e))?;
        let (h,_)=read_header(&mut input)?;
        let entries=read_entries(&mut input,&h,report.manifest.bin.size)?;
        let mut bin=OpenOptions::new().write(true).create_new(true).open(&bin_temp).map_err(|e|err("Create temporary BIN",e))?;
        let mut bin_hash=Sha256::new();
        for (n,entry) in entries.iter().enumerate() {
            checked_cancel(cancel)?;
            input.seek(SeekFrom::Start(entry.compressed_off)).map_err(|e|err("Seek chunk",e))?;
            let mut compressed=vec![0u8;entry.compressed_len as usize];
            input.read_exact(&mut compressed).map_err(|e|err("Read chunk",e))?;
            let raw=compression::decode_chunk_codec(&compressed,entry.source_len as usize,&entry.hash,entry.codec).map_err(|e|format!("Chunk {} {e}",n+1))?;
            bin.write_all(&raw).map_err(|e|err("Write BIN",e))?;
            bin_hash.update(&raw);
            progress(Progress{stage:"Extracting".into(),done:n as u64+1,total:h.chunks,bytes_written:entry.source_off+entry.source_len as u64,elapsed_seconds:start.elapsed().as_secs()});
        }
        bin.sync_all().map_err(|e|err("Sync BIN",e))?; drop(bin);
        if hex::encode(bin_hash.finalize())!=report.manifest.bin.sha256 { return Err("Extracted BIN SHA-256 mismatch".into()); }
        input.seek(SeekFrom::Start(h.cue_off)).map_err(|e|err("Seek CUE",e))?;
        let mut cue=OpenOptions::new().write(true).create_new(true).open(&cue_temp).map_err(|e|err("Create temporary CUE",e))?;
        let mut cue_hash=Sha256::new(); let mut left=h.cue_len; let mut buf=[0u8;8192];
        while left>0 { checked_cancel(cancel)?; let n=(left as usize).min(buf.len()); input.read_exact(&mut buf[..n]).map_err(|e|err("Read CUE",e))?; cue.write_all(&buf[..n]).map_err(|e|err("Write CUE",e))?; cue_hash.update(&buf[..n]); left-=n as u64; }
        cue.sync_all().map_err(|e|err("Sync CUE",e))?; drop(cue);
        if hex::encode(cue_hash.finalize())!=report.manifest.cue.sha256 { return Err("Extracted CUE SHA-256 mismatch".into()); }
        checked_cancel(cancel)?;
        fs::rename(&bin_temp,&bin_path).map_err(|e|err("Finalize BIN",e))?;
        if let Err(e)=fs::rename(&cue_temp,&cue_path) { let _=fs::remove_file(&bin_path); return Err(err("Finalize CUE",e)); }
        Ok(())
    })();
    if result.is_err() { let _=fs::remove_file(&bin_temp); let _=fs::remove_file(&cue_temp); }
    result.map(|_|report)
}
