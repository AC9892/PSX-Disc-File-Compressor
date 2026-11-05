use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, path::{Path, PathBuf}, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscSource {
    pub path: String, pub platform: String, pub media: String, pub format: String,
    pub serial: Option<String>, pub volume_id: Option<String>, pub title: Option<String>,
    pub files: Vec<String>, pub original_size: u64, pub confidence: String,
    pub restored_bin_name: Option<String>, pub compressed_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFile { pub name: String, pub size: u64, pub sha256: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChdManifest { pub version: u32, pub disc: DiscSource, pub files: Vec<StoredFile>, pub chd_sha256: String, pub chd_size: u64, pub exact_restoration_verified: bool, pub cue_bytes: Option<Vec<u8>> }

#[derive(Debug, Clone, Serialize)]
pub struct ChdReport { pub manifest: ChdManifest, pub verified: bool, pub warning: Option<String> }

fn safe_name(path: &Path) -> Result<String, String> {
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("Invalid filename")?;
    if name.is_empty() || name == "." || name == ".." || name.ends_with([' ', '.']) || name.chars().any(|c| c.is_control() || "/\\:<>\"|?*".contains(c)) { return Err("Unsafe filename".into()); }
    Ok(name.into())
}

fn hash(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new(); let mut buf = [0u8; 1024 * 1024];
    loop { let n = f.read(&mut buf).map_err(|e| e.to_string())?; if n == 0 { break; } h.update(&buf[..n]); }
    Ok(hex::encode(h.finalize()))
}

fn sidecar(path: &Path) -> PathBuf { path.with_extension("chd.json") }

fn chdman_binary() -> PathBuf {
    if let Ok(exe)=std::env::current_exe() {
        if let Some(folder)=exe.parent() { let adjacent=folder.join("chdman.exe"); if adjacent.is_file() { return adjacent } }
    }
    PathBuf::from("chdman")
}

fn chdman(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = Command::new(chdman_binary()).args(args).output().map_err(|e| format!("chdman.exe must be beside this app or on PATH: {e}"))?;
    if output.status.success() { Ok(()) } else { Err(format!("chdman failed: {}", String::from_utf8_lossy(&output.stderr))) }
}

fn chd_media(path: &Path) -> Result<&'static str,String> {
    let output=Command::new(chdman_binary()).args(["info","-i"]).arg(path).output().map_err(|e|format!("chdman.exe must be beside this app or on PATH: {e}"))?;
    if !output.status.success() { return Err("chdman could not read CHD metadata".into()) }
    let info=String::from_utf8_lossy(&output.stdout).to_ascii_uppercase();
    if info.contains("CHTR") || info.contains("CHCD") || info.contains("CDROM_TRACK") { Ok("CD-ROM") }
    else { Ok("DVD-ROM") }
}

fn read_block(file: &mut File, sector: u64, size: u64) -> Option<[u8; 2048]> {
    file.seek(SeekFrom::Start(sector.checked_mul(size)?)).ok()?;
    let mut b = [0u8; 2048]; file.read_exact(&mut b).ok()?; Some(b)
}

fn iso_metadata(path: &Path, sector_size: u64, data_start: u64) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(mut f) = File::open(path) else { return (None,None,None) };
    let Some(pvd) = read_block(&mut f, data_start + 16, sector_size) else { return (None,None,None) };
    if &pvd[1..6] != b"CD001" || pvd[0] != 1 { return (None,None,None) }
    let volume = String::from_utf8_lossy(&pvd[40..72]).trim().to_string();
    let root = &pvd[156..];
    let extent = u32::from_le_bytes(root[2..6].try_into().unwrap()) as u64;
    let len = u32::from_le_bytes(root[10..14].try_into().unwrap()) as usize;
    if len > 1024 * 1024 { return (Some(volume),None,None) }
    let mut dir = vec![0u8; len.div_ceil(2048) * 2048];
    for (n, chunk) in dir.chunks_mut(2048).enumerate() { let Some(b) = read_block(&mut f, data_start + extent + n as u64, sector_size) else { return (Some(volume),None,None) }; chunk.copy_from_slice(&b); }
    let mut at=0;
    while at < len {
        let n=dir[at] as usize;
        if n == 0 { at=(at/2048+1)*2048; continue }
        if n < 34 || at+n > len { break }
        let e=&dir[at..at+n]; let name_len=e[32] as usize;
        if 33+name_len <= n && String::from_utf8_lossy(&e[33..33+name_len]).split(';').next().unwrap_or("").eq_ignore_ascii_case("SYSTEM.CNF") {
            let sector=u32::from_le_bytes(e[2..6].try_into().unwrap()) as u64;
            let count=u32::from_le_bytes(e[10..14].try_into().unwrap()) as usize;
            if count > 65536 { break }
            let mut data=Vec::new();
            for k in 0..count.div_ceil(2048) { let Some(b)=read_block(&mut f,data_start+sector+k as u64,sector_size) else { break }; data.extend_from_slice(&b); }
            let txt=String::from_utf8_lossy(&data[..data.len().min(count)]);
            for line in txt.lines() { if let Some((key,val))=line.split_once('=') { if key.trim().eq_ignore_ascii_case("BOOT2") || key.trim().eq_ignore_ascii_case("BOOT") { let boot=val.trim().to_string(); let exe=boot.rsplit(['\\','/']).next().unwrap_or("").split(';').next().unwrap_or(""); let serial=px1_core::metadata::serial_from_executable(exe).map(|(s,_)|s); return (Some(volume),serial,Some(format!("{}={boot}",key.trim().to_ascii_uppercase()))); } } }
            break
        }
        at+=n;
    }
    (Some(volume),None,None)
}

pub fn inspect(path: &Path, selected_bin: Option<&Path>) -> Result<DiscSource,String> {
    if !path.is_file() { return Err("Source image is missing".into()) }
    let ext=path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "cue" {
        let cue=px1_core::cue::parse_with_bin(path,selected_bin)?;
        let meta=px1_core::metadata::detect(&cue.bin_path,cue.data_track_start_sector);
        let platform=meta.platform.as_deref().unwrap_or("Unknown PlayStation Disc");
        return Ok(DiscSource { path:path.display().to_string(), platform:platform.into(), media:"CD-ROM".into(), format:"BIN/CUE".into(), serial:meta.serial, volume_id:meta.volume_id, title:None, files:vec![cue.bin_path.display().to_string(),path.display().to_string()], original_size:fs::metadata(&cue.bin_path).map_err(|e|e.to_string())?.len()+cue.cue_bytes.len() as u64, confidence:if meta.platform.is_some() {"disc contents"} else {"unidentified"}.into(), restored_bin_name:Some(cue.referenced_bin_name), compressed_size:None });
    }
    if ext == "bin" { return Err("Select the CUE for a BIN/CUE disc, not the BIN alone".into()) }
    if ext == "chd" {
        let mut magic=[0u8;8]; File::open(path).and_then(|mut f| f.read_exact(&mut magic)).map_err(|e|e.to_string())?;
        if &magic != b"MComprHD" { return Err("Invalid CHD header".into()) }
        if sidecar(path).exists() { let saved=read_manifest(path)?; let mut disc=saved.disc; disc.path=path.display().to_string(); disc.format="CHD".into(); disc.original_size=saved.files.iter().map(|f|f.size).sum(); disc.compressed_size=Some(fs::metadata(path).map_err(|e|e.to_string())?.len()); disc.confidence="recorded by companion metadata".into(); return Ok(disc) }
        return Ok(DiscSource { path:path.display().to_string(), platform:"Unknown PlayStation Disc".into(), media:chd_media(path).unwrap_or("Unknown").into(), format:"CHD".into(), serial:None, volume_id:None, title:None, files:vec![path.display().to_string()], original_size:fs::metadata(path).map_err(|e|e.to_string())?.len(), confidence:"unidentified; no original-hash metadata".into(), restored_bin_name:None, compressed_size:Some(fs::metadata(path).map_err(|e|e.to_string())?.len()) });
    }
    let len=fs::metadata(path).map_err(|e|e.to_string())?.len();
    if len == 0 || len % 2048 != 0 { return Err("Not a complete 2048-byte-sector DVD ISO".into()) }
    let (vol,serial,boot)=iso_metadata(path,2048,0);
    let platform=if boot.as_deref().is_some_and(|b|b.starts_with("BOOT2=")) { "PlayStation 2" } else if boot.is_some() { "PlayStation" } else { "Unknown PlayStation Disc" };
    Ok(DiscSource { path:path.display().to_string(), platform:platform.into(), media:if platform == "PlayStation" {"CD-ROM"} else {"DVD-ROM"}.into(), format:"ISO".into(), serial, volume_id:vol, title:None, files:vec![path.display().to_string()], original_size:len, confidence:if boot.is_some() {"disc contents"} else {"unidentified"}.into(), restored_bin_name:None, compressed_size:None })
}

fn stored(path: &Path) -> Result<StoredFile,String> { Ok(StoredFile { name:safe_name(path)?, size:fs::metadata(path).map_err(|e|e.to_string())?.len(), sha256:hash(path)? }) }

fn sanitized_disc(source: &DiscSource, input: &Path, files: &[StoredFile]) -> Result<DiscSource,String> {
    let mut disc=source.clone();
    disc.path=safe_name(input)?;
    disc.files=files.iter().map(|f|f.name.clone()).collect();
    Ok(disc)
}

pub fn compress(selected: &DiscSource, output: &Path) -> Result<ChdReport,String> {
    let input=Path::new(&selected.path);
    let mut source=inspect(input,None)?;
    if source.platform == "PlayStation" { return Err("This is a PS1 disc; use the existing PX1 workflow".into()) }
    if source.platform == "Unknown PlayStation Disc" && selected.platform == "PlayStation 2" { source.platform="PlayStation 2".into(); source.confidence="manual selection".into(); }
    if source.platform != "PlayStation 2" { return Err("Select PlayStation 2 manually for an unidentified disc before compression".into()) }
    if source.format == "CHD" { return Err("Existing CHD is already compressed".into()) }
    if source.files.is_empty() { return Err("No source files".into()) }
    if source.format == "BIN/CUE" && source.restored_bin_name.as_deref() != Path::new(&source.files[0]).file_name().and_then(|n|n.to_str()) { return Err("For PS2 CHD creation, place the BIN beside the CUE under the filename its FILE line references".into()) }
    if output.exists() || sidecar(output).exists() { return Err("Output CHD or metadata already exists".into()) }
    let mut files=Vec::new(); for name in &source.files { files.push(stored(Path::new(name))?); }
    if let Some(name)=&source.restored_bin_name { if safe_name(Path::new(name))? != *name { return Err("Unsafe CUE BIN name".into()) } files[0].name=name.clone(); }
    let cue_bytes=if source.format == "BIN/CUE" { Some(fs::read(input).map_err(|e|e.to_string())?) } else { None };
    let temp=output.with_extension("chd.partial"); if temp.exists() { return Err("Temporary CHD already exists".into()) }
    let kind=if source.format == "ISO" { "createdvd" } else { "createcd" };
    let result=(|| {
        chdman(&[kind.as_ref(),"-i".as_ref(),input.as_os_str(),"-o".as_ref(),temp.as_os_str()])?;
        chdman(&["verify".as_ref(),"-i".as_ref(),temp.as_os_str()])?;
        let chd_sha256=hash(&temp)?;
        let chd_size=fs::metadata(&temp).map_err(|e|e.to_string())?.len();
        let disc_for_manifest=sanitized_disc(&source,input,&files)?;
        let mut manifest=ChdManifest { version:1, disc:disc_for_manifest, files, chd_sha256, chd_size, exact_restoration_verified:false, cue_bytes };
        let check_dir=output.with_extension("chd.verify-temp");
        fs::create_dir(&check_dir).map_err(|e|format!("Cannot create verification folder: {e}"))?;
        let exact=(|| { extract_raw(&temp,&manifest,&check_dir)?; compare_files(&manifest,&check_dir) })();
        for item in &manifest.files { let _=fs::remove_file(check_dir.join(&item.name)); }
        let _=fs::remove_dir(&check_dir);
        manifest.exact_restoration_verified=exact?;
        let mut f=OpenOptions::new().write(true).create_new(true).open(sidecar(output)).map_err(|e|e.to_string())?;
        f.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        fs::rename(&temp,output).map_err(|e|e.to_string())?;
        let verified=manifest.exact_restoration_verified;
        Ok(ChdReport { manifest, verified, warning:if verified {None} else {Some("CHD integrity passed, but extracted bytes differ from the original. Keep the source files.".into())} })
    })();
    if result.is_err() { let _=fs::remove_file(&temp); }
    result
}

pub fn read_manifest(chd: &Path) -> Result<ChdManifest,String> {
    let manifest:ChdManifest=serde_json::from_slice(&fs::read(sidecar(chd)).map_err(|e|format!("Cannot read CHD sidecar: {e}"))?).map_err(|e|format!("Invalid CHD sidecar: {e}"))?;
    if manifest.version != 1 || !matches!(manifest.disc.format.as_str(),"ISO"|"BIN/CUE") || manifest.files.len() != if manifest.disc.format == "ISO" {1} else {2} { return Err("Unsupported CHD sidecar layout".into()) }
    for item in &manifest.files { if safe_name(Path::new(&item.name))? != item.name { return Err("Unsafe CHD sidecar filename".into()) } }
    Ok(manifest)
}

pub fn verify(chd: &Path) -> Result<ChdReport,String> {
    if !sidecar(chd).exists() { return verify_unmanaged(chd) }
    let manifest=read_manifest(chd)?;
    if hash(chd)? != manifest.chd_sha256 { return Err("CHD SHA-256 differs from recorded source".into()) }
    chdman(&["verify".as_ref(),"-i".as_ref(),chd.as_os_str()])?;
    Ok(ChdReport { manifest, verified:true, warning:Some("CHD integrity passed; original bytes require extraction and SHA-256 comparison.".into()) })
}

fn verify_unmanaged(chd: &Path) -> Result<ChdReport,String> {
    chdman(&["verify".as_ref(),"-i".as_ref(),chd.as_os_str()])?;
    let disc=inspect(chd,None)?;
    let manifest=ChdManifest { version:1, disc, files:Vec::new(), chd_sha256:hash(chd)?, chd_size:fs::metadata(chd).map_err(|e|e.to_string())?.len(), exact_restoration_verified:false, cue_bytes:None };
    Ok(ChdReport { manifest, verified:true, warning:Some("CHD integrity passed. Without original hashes, byte-for-byte equivalence cannot be proven.".into()) })
}

pub fn extract(chd: &Path, out_dir: &Path) -> Result<ChdReport,String> {
    if !sidecar(chd).exists() { return extract_unmanaged(chd,out_dir) }
    let mut manifest=read_manifest(chd)?;
    if !out_dir.is_dir() { return Err("Output folder is missing".into()) }
    for item in &manifest.files { if item.name != safe_name(Path::new(&item.name))? || out_dir.join(&item.name).exists() { return Err("Unsafe or existing output file".into()) } }
    if hash(chd)? != manifest.chd_sha256 { return Err("CHD SHA-256 mismatch".into()) }
    extract_raw(chd,&manifest,out_dir)?;
    let exact=compare_files(&manifest,out_dir)?;
    manifest.exact_restoration_verified=exact;
    Ok(ChdReport { manifest, verified:exact, warning:if exact {None} else {Some("Restored bytes differ from the original hashes; keep the original images.".into())} })
}

fn extract_unmanaged(chd: &Path, out_dir: &Path) -> Result<ChdReport,String> {
    if !out_dir.is_dir() { return Err("Output folder is missing".into()) }
    let report=verify_unmanaged(chd)?;
    let stem=chd.file_stem().and_then(|s|s.to_str()).ok_or("Invalid CHD name")?;
    if safe_name(Path::new(stem))? != stem { return Err("Unsafe CHD name".into()) }
    let media=chd_media(chd)?;
    if media == "CD-ROM" {
        let cue=out_dir.join(format!("{stem}.cue")); let bin=out_dir.join(format!("{stem}.bin"));
        if cue.exists() || bin.exists() { return Err("Restored output already exists".into()) }
        chdman(&["extractcd".as_ref(),"-i".as_ref(),chd.as_os_str(),"-o".as_ref(),cue.as_os_str(),"-ob".as_ref(),bin.as_os_str()])?;
    } else {
        let iso=out_dir.join(format!("{stem}.iso")); if iso.exists() { return Err("Restored output already exists".into()) }
        chdman(&["extractdvd".as_ref(),"-i".as_ref(),chd.as_os_str(),"-o".as_ref(),iso.as_os_str()])?;
    }
    Ok(ChdReport { manifest:report.manifest, verified:false, warning:Some("Extracted CHD without original hashes; exact source filenames and bytes cannot be proven.".into()) })
}

fn extract_raw(chd: &Path, manifest: &ChdManifest, out_dir: &Path) -> Result<(),String> {
    let output=out_dir.join(&manifest.files[0].name);
    if manifest.disc.format == "ISO" { chdman(&["extractdvd".as_ref(),"-i".as_ref(),chd.as_os_str(),"-o".as_ref(),output.as_os_str()])?; }
    else {
        let cue=out_dir.join(&manifest.files[1].name);
        chdman(&["extractcd".as_ref(),"-i".as_ref(),chd.as_os_str(),"-o".as_ref(),cue.as_os_str(),"-ob".as_ref(),output.as_os_str()])?;
        if let Some(bytes)=&manifest.cue_bytes { fs::write(cue,bytes).map_err(|e|e.to_string())?; }
    }
    Ok(())
}

fn compare_files(manifest: &ChdManifest, out_dir: &Path) -> Result<bool,String> {
    for item in &manifest.files { let p=out_dir.join(&item.name); if !p.exists() || fs::metadata(&p).map_err(|e|e.to_string())?.len()!=item.size || hash(&p)? != item.sha256 { return Ok(false) } }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime,UNIX_EPOCH};

    fn fixture(boot: &str) -> PathBuf {
        let id=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path=std::env::temp_dir().join(format!("ps2-detect-{}-{id}.img",std::process::id()));
        let mut image=vec![0u8; 32*2048];
        let pvd=&mut image[16*2048..17*2048]; pvd[0]=1; pvd[1..6].copy_from_slice(b"CD001"); pvd[6]=1;
        pvd[40..47].copy_from_slice(b"TESTVOL"); pvd[156]=34;
        pvd[158..162].copy_from_slice(&20u32.to_le_bytes());
        pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
        let dir=&mut image[20*2048..21*2048]; dir[0]=46; dir[2..6].copy_from_slice(&21u32.to_le_bytes());
        let cnf=format!("{boot} = cdrom0:\\SLUS_216.78;1\n"); dir[10..14].copy_from_slice(&(cnf.len() as u32).to_le_bytes());
        dir[32]=12; dir[33..45].copy_from_slice(b"SYSTEM.CNF;1");
        image[21*2048..21*2048+cnf.len()].copy_from_slice(cnf.as_bytes());
        fs::write(&path,image).unwrap(); path
    }

    #[test]
    fn ps2_detection_uses_boot2_inside_disc_not_extension() {
        let path=fixture("BOOT2");
        let found=inspect(&path,None).unwrap();
        assert_eq!(found.platform,"PlayStation 2"); assert_eq!(found.media,"DVD-ROM");
        assert_eq!(found.serial.as_deref(),Some("SLUS-21678"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unknown_disc_requires_manual_selection() {
        let path=fixture("NOTBOOT");
        let found=inspect(&path,None).unwrap(); assert_eq!(found.platform,"Unknown PlayStation Disc");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_incomplete_and_fake_chd() {
        let path=std::env::temp_dir().join(format!("ps2-incomplete-{}.iso",std::process::id()));
        fs::write(&path,[0u8;13]).unwrap(); assert!(inspect(&path,None).is_err()); fs::remove_file(path).unwrap();
        let path=std::env::temp_dir().join(format!("ps2-fake-{}.chd",std::process::id()));
        fs::write(&path,[0u8;16]).unwrap(); assert!(inspect(&path,None).is_err()); fs::remove_file(path).unwrap();
    }

    #[test]
    fn sidecar_uses_file_labels_without_source_directories() {
        let source=DiscSource { path:"X:\\private\\disc.iso".into(), platform:"PlayStation 2".into(), media:"DVD-ROM".into(), format:"ISO".into(), serial:None, volume_id:None, title:None, files:vec!["X:\\private\\disc.iso".into()], original_size:2048, confidence:"disc contents".into(), restored_bin_name:None, compressed_size:None };
        let files=vec![StoredFile { name:"disc.iso".into(), size:2048, sha256:"abc".into() }];
        let clean=sanitized_disc(&source,Path::new(&source.path),&files).unwrap();
        let json=serde_json::to_string(&clean).unwrap();
        assert!(!json.contains("private")); assert!(json.contains("disc.iso"));
    }

    #[test]
    fn corrupted_chd_fails_recorded_hash_before_external_verify() {
        let id=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path=std::env::temp_dir().join(format!("ps2-corrupt-{}-{id}.chd",std::process::id()));
        fs::write(&path,b"MComprHDcorrupted").unwrap();
        let disc=DiscSource { path:"disc.chd".into(), platform:"PlayStation 2".into(), media:"DVD-ROM".into(), format:"ISO".into(), serial:None, volume_id:None, title:None, files:vec!["disc.iso".into()], original_size:2048, confidence:"disc contents".into(), restored_bin_name:None, compressed_size:None };
        let manifest=ChdManifest { version:1, disc, files:vec![StoredFile { name:"disc.iso".into(), size:2048, sha256:"00".repeat(32) }], chd_sha256:"00".repeat(32), chd_size:16, exact_restoration_verified:false, cue_bytes:None };
        fs::write(sidecar(&path),serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(verify(&path).unwrap_err().contains("SHA-256"));
        fs::remove_file(sidecar(&path)).unwrap(); fs::remove_file(path).unwrap();
    }

    #[test]
    fn optional_real_ps2_iso_inspection() {
        let Ok(path)=std::env::var("PS2_TEST_ISO") else { return };
        let found=inspect(Path::new(&path),None).unwrap();
        assert_eq!(found.platform,"PlayStation 2");
        assert_eq!(found.media,"DVD-ROM");
        assert_eq!(found.format,"ISO");
        assert!(found.serial.is_some());
    }

    #[test]
    fn optional_real_chdman_dvd_roundtrip() {
        if std::env::var_os("PS2_CHDMAN_TEST").is_none() { return }
        let source=fixture("BOOT2");
        let id=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir=std::env::temp_dir().join(format!("ps2-chd-test-{}-{id}",std::process::id()));
        fs::create_dir(&dir).unwrap();
        let chd=dir.join("disc.chd");
        let inspected=inspect(&source,None).unwrap();
        let compressed=compress(&inspected,&chd).unwrap();
        assert!(compressed.verified,"{}",compressed.warning.unwrap_or_default());
        assert!(verify(&chd).unwrap().verified);
        let restored=extract(&chd,&dir).unwrap();
        assert!(restored.verified,"{}",restored.warning.unwrap_or_default());
        let original_hash=hash(&source).unwrap();
        assert_eq!(hash(&dir.join(safe_name(&source).unwrap())).unwrap(),original_hash);
        fs::remove_file(dir.join(safe_name(&source).unwrap())).unwrap();
        fs::remove_file(sidecar(&chd)).unwrap();
        fs::remove_file(chd).unwrap();
        fs::remove_dir(dir).unwrap();
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn optional_real_chdman_cd_roundtrip() {
        if std::env::var_os("PS2_CHDMAN_TEST").is_none() { return }
        let iso=fixture("BOOT2"); let bytes=fs::read(&iso).unwrap();
        let id=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir=std::env::temp_dir().join(format!("ps2-cd-test-{}-{id}",std::process::id()));
        fs::create_dir(&dir).unwrap();
        let bin=dir.join("source.bin"); let cue=dir.join("source.cue");
        let mut raw=vec![0u8;bytes.len()/2048*2352];
        for (i,chunk) in bytes.chunks_exact(2048).enumerate() { let sector=&mut raw[i*2352..(i+1)*2352]; sector[1..11].fill(0xff); sector[15]=2; sector[24..24+2048].copy_from_slice(chunk); }
        fs::write(&bin,&raw).unwrap();
        fs::write(&cue,b"FILE \"source.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n").unwrap();
        let inspected=inspect(&cue,None).unwrap(); assert_eq!(inspected.platform,"PlayStation 2");
        let chd=dir.join("source.chd");
        let compressed=compress(&inspected,&chd).unwrap();
        assert!(compressed.verified,"{}",compressed.warning.unwrap_or_default());
        let restored=dir.join("restore"); fs::create_dir(&restored).unwrap();
        let report=extract(&chd,&restored).unwrap(); assert!(report.verified,"{}",report.warning.unwrap_or_default());
        assert_eq!(hash(&bin).unwrap(),hash(&restored.join("source.bin")).unwrap());
        assert_eq!(hash(&cue).unwrap(),hash(&restored.join("source.cue")).unwrap());
        fs::remove_file(restored.join("source.bin")).unwrap(); fs::remove_file(restored.join("source.cue")).unwrap(); fs::remove_dir(restored).unwrap();
        fs::remove_file(sidecar(&chd)).unwrap(); fs::remove_file(chd).unwrap(); fs::remove_file(bin).unwrap(); fs::remove_file(cue).unwrap(); fs::remove_dir(dir).unwrap(); fs::remove_file(iso).unwrap();
    }

    #[test]
    fn optional_full_ps2_iso_chd_roundtrip() {
        let (Ok(path),Some(_))=(std::env::var("PS2_TEST_ISO"),std::env::var_os("PS2_FULL_CHD_TEST")) else { return };
        let source=Path::new(&path); let inspected=inspect(source,None).unwrap();
        let id=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir=std::env::temp_dir().join(format!("ps2-full-test-{}-{id}",std::process::id()));
        fs::create_dir(&dir).unwrap();
        let chd=dir.join("test.chd");
        let result=(|| -> Result<u64,String> {
            let compressed=compress(&inspected,&chd)?;
            if !compressed.verified { return Err(compressed.warning.unwrap_or("CHD did not restore exactly".into())) }
            verify(&chd)?;
            let restored=extract(&chd,&dir)?;
            if !restored.verified { return Err(restored.warning.unwrap_or("Restore hash mismatch".into())) }
            Ok(compressed.manifest.chd_size)
        })();
        let _=fs::remove_file(dir.join(safe_name(source).unwrap()));
        let _=fs::remove_file(sidecar(&chd));
        let _=fs::remove_file(&chd);
        let _=fs::remove_file(chd.with_extension("chd.partial"));
        let _=fs::remove_dir(&dir);
        let chd_size=result.unwrap();
        println!("Real PS2 ISO round trip passed; original={} bytes, CHD={chd_size} bytes",inspected.original_size);
    }
}
