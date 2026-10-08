use super::*;
use std::{cell::RefCell, fs, io::{Read,Seek,SeekFrom,Write}, sync::atomic::{AtomicBool,Ordering}};
use sha2::{Digest,Sha256};

fn write_sector(image: &mut [u8], number: usize, payload: &[u8]) {
    let base=number*2352;
    image[base]=0;
    image[base+1..base+11].fill(0xff);
    image[base+11]=0;
    image[base+15]=2;
    image[base+24..base+24+payload.len()].copy_from_slice(payload);
}

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let bin=dir.join("20261007_021434.BIN");
    let cue=dir.join("20261007_021434.CUE");
    let mut image=vec![0u8;2352*40];
    for (i,b) in image.iter_mut().enumerate() { *b=(i as u8).wrapping_mul(31); }
    let mut pvd=[0u8;2048];
    pvd[0]=1; pvd[1..6].copy_from_slice(b"CD001"); pvd[6]=1;
    pvd[40..49].copy_from_slice(b"TEST_DISC");
    pvd[156]=34;
    pvd[158..162].copy_from_slice(&20u32.to_le_bytes());
    pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
    write_sector(&mut image,16,&pvd);
    let mut root=[0u8;2048];
    let name=b"SYSTEM.CNF;1";
    root[0]=46; root[2..6].copy_from_slice(&21u32.to_le_bytes());
    root[10..14].copy_from_slice(&32u32.to_le_bytes());
    root[32]=name.len() as u8; root[33..33+name.len()].copy_from_slice(name);
    write_sector(&mut image,20,&root);
    let mut cnf=[0u8;2048];
    let boot=b"BOOT = cdrom:\\SLUS_008.92;1\r\n";
    cnf[..boot.len()].copy_from_slice(boot);
    write_sector(&mut image,21,&cnf);
    fs::write(&bin,image).unwrap();
    fs::write(&cue,b"FILE \"20261007_021434.BIN\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n").unwrap();
    cue
}

#[test]
fn generic_names_detect_disc_and_roundtrip_exactly() {
    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    let bin=dir.path().join("20261007_021434.BIN");
    let original_bin=fs::read(&bin).unwrap();
    let original_cue=fs::read(&cue).unwrap();
    let info=container::source_info(&cue).unwrap();
    assert_eq!(info.disc.serial.as_deref(),Some("SLUS-00892"));
    assert_eq!(info.disc.region.as_deref(),Some("USA"));
    assert_eq!(info.disc.title,None);
    let px=dir.path().join("preserved.px1");
    let cancel=AtomicBool::new(false);
    let report=compress(&cue,&px,1,Some("Descriptive title".into()),Some(1),&cancel,|_|{}).unwrap();
    assert!(report.container_verified && report.bin_verified && report.cue_verified);
    assert_eq!(report.container_sha256.len(),64);
    assert_eq!(report.manifest.disc.title.as_deref(),Some("Descriptive title"));
    verify(&px,&cancel,|_|{}).unwrap();
    let extracted=dir.path().join("out"); fs::create_dir(&extracted).unwrap();
    extract(&px,&extracted,&cancel,|_|{}).unwrap();
    assert_eq!(fs::read(extracted.join("20261007_021434.BIN")).unwrap(),original_bin);
    assert_eq!(fs::read(extracted.join("20261007_021434.CUE")).unwrap(),original_cue);
    assert_eq!(fs::read(bin).unwrap(),original_bin);
    assert_eq!(fs::read(cue).unwrap(),original_cue);
}

#[test]
fn invalid_sources_outputs_and_corruption_are_rejected() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path());
    let cancel=AtomicBool::new(false); let px=dir.path().join("disc.px1");
    fs::write(&px,b"existing").unwrap();
    assert!(compress(&cue,&px,1,None,None,&cancel,|_|{}).is_err());
    fs::remove_file(&px).unwrap();
    compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    let mut f=fs::OpenOptions::new().read(true).write(true).open(&px).unwrap();
    let len=f.metadata().unwrap().len(); f.seek(SeekFrom::Start(len-40)).unwrap(); f.write_all(b"BAD!").unwrap(); drop(f);
    assert!(verify(&px,&cancel,|_|{}).is_err());
    fs::write(dir.path().join("broken.cue"),b"FILE \"missing.bin\" BINARY\n TRACK 01 MODE2/2352\n").unwrap();
    assert!(container::source_info(&dir.path().join("broken.cue")).is_err());
    fs::write(dir.path().join("multi.cue"),b"FILE \"a.bin\" BINARY\nFILE \"b.bin\" BINARY\nTRACK 01 MODE2/2352\n").unwrap();
    assert!(container::source_info(&dir.path().join("multi.cue")).is_err());
    fs::write(dir.path().join("empty.px1"),[]).unwrap();
    assert!(inspect(&dir.path().join("empty.px1")).is_err());
}

#[test]
fn cancellation_leaves_no_output() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path()); let px=dir.path().join("cancel.px1");
    let cancel=AtomicBool::new(true);
    assert!(compress(&cue,&px,1,None,None,&cancel,|_|{}).is_err());
    assert!(!px.exists());
    assert!(!dir.path().join("cancel.px1.tmp").exists());
    cancel.store(false,Ordering::SeqCst);
}

#[test]
fn multiple_chunks_and_existing_extraction_are_safe() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path());
    let bin=dir.path().join("20261007_021434.BIN");
    let two_chunk_size = (4*1024*1024 / 2352 + 1) * 2352;
    fs::OpenOptions::new().write(true).open(&bin).unwrap().set_len(two_chunk_size).unwrap();
    let px=dir.path().join("large.px1"); let cancel=AtomicBool::new(false);
    let report=compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    assert_eq!(report.chunks_verified,2);
    let out=dir.path().join("out"); fs::create_dir(&out).unwrap();
    fs::write(out.join("20261007_021434.BIN"),b"existing").unwrap();
    assert!(extract(&px,&out,&cancel,|_|{}).is_err());
    assert_eq!(fs::read(out.join("20261007_021434.BIN")).unwrap(),b"existing");
    fs::remove_file(out.join("20261007_021434.BIN")).unwrap();
    extract(&px,&out,&cancel,|_|{}).unwrap();
    assert_eq!(fs::read(&bin).unwrap(),fs::read(out.join("20261007_021434.BIN")).unwrap());
}

#[test]
fn malformed_header_and_manifest_fail_cleanly() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path());
    let px=dir.path().join("valid.px1"); let cancel=AtomicBool::new(false);
    compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    let valid=fs::read(&px).unwrap();
    let mut bad=valid.clone(); bad[0]=b'X'; fs::write(dir.path().join("magic.px1"),bad).unwrap();
    assert!(verify(&dir.path().join("magic.px1"),&cancel,|_|{}).unwrap_err().contains("magic"));
    let mut bad=valid.clone(); bad[96]=b'!'; fs::write(dir.path().join("manifest.px1"),bad).unwrap();
    assert!(verify(&dir.path().join("manifest.px1"),&cancel,|_|{}).unwrap_err().contains("manifest"));
    fs::write(dir.path().join("truncated.px1"),&valid[..valid.len()-1]).unwrap();
    assert!(verify(&dir.path().join("truncated.px1"),&cancel,|_|{}).is_err());
    let mut bad=valid.clone(); bad[40..48].copy_from_slice(&(8u64*1024*1024+1).to_le_bytes());
    fs::write(dir.path().join("oversized-cue.px1"),bad).unwrap();
    assert!(verify(&dir.path().join("oversized-cue.px1"),&cancel,|_|{}).unwrap_err().contains("CUE is too large"));
}

#[test]
fn rejects_non_sector_aligned_bin() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path());
    let bin=dir.path().join("20261007_021434.BIN");
    fs::OpenOptions::new().write(true).open(bin).unwrap().set_len(2353).unwrap();
    assert!(container::source_info(&cue).unwrap_err().contains("2352-byte-sector"));
}

#[test]
fn cancellation_after_first_chunk_removes_partial_px1() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path());
    let bin=dir.path().join("20261007_021434.BIN");
    let original_cue=fs::read(&cue).unwrap();
    let two_chunk_size=(4*1024*1024 / 2352 + 1) * 2352;
    fs::OpenOptions::new().write(true).open(&bin).unwrap().set_len(two_chunk_size).unwrap();
    let original_size=fs::metadata(&bin).unwrap().len();
    let px=dir.path().join("cancel-during-compression.px1");
    let cancel=AtomicBool::new(false);
    let error=compress(&cue,&px,1,None,None,&cancel,|progress| {
        if progress.stage=="Compressing" && progress.done==1 { cancel.store(true,Ordering::SeqCst); }
    }).unwrap_err();
    assert!(error.contains("cancelled"));
    assert!(!px.exists());
    assert!(!dir.path().join("cancel-during-compression.px1.tmp").exists());
    assert_eq!(fs::metadata(&bin).unwrap().len(),original_size);
    assert_eq!(fs::read(&cue).unwrap(),original_cue);
}

#[test]
fn wrong_bin_hash_fails_with_valid_container_footer() {
    let dir=tempfile::tempdir().unwrap(); let cue=source(dir.path());
    let px=dir.path().join("wrong-hash.px1"); let cancel=AtomicBool::new(false);
    let report=compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    let mut bytes=fs::read(&px).unwrap();
    let expected=report.manifest.bin.sha256.as_bytes();
    let at=bytes.windows(expected.len()).position(|part|part==expected).unwrap();
    bytes[at]=if bytes[at]==b'0' { b'1' } else { b'0' };
    let footer_at=bytes.len()-32;
    let digest=Sha256::digest(&bytes[..footer_at]);
    bytes[footer_at..].copy_from_slice(&digest);
    fs::write(&px,bytes).unwrap();
    assert!(verify(&px,&cancel,|_|{}).unwrap_err().contains("BIN SHA-256 mismatch"));
}

#[test]
fn cue_track_details_and_invalid_indices() {
    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    let valid=b"FILE \"20261007_021434.BIN\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n  TRACK 02 AUDIO\r\n    INDEX 00 00:00:20\r\n    INDEX 01 00:00:30\r\n";
    fs::write(&cue,valid).unwrap();
    let info=container::source_info(&cue).unwrap();
    assert_eq!((info.track_count,info.data_track_count),(2,1));

    for invalid in [
        b"FILE \"20261007_021434.BIN\" BINARY\nTRACK XX MODE2/2352\nINDEX 01 00:00:00\n".as_slice(),
        b"FILE \"20261007_021434.BIN\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:75:00\n",
        b"FILE \"20261007_021434.BIN\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 nonsense\n",
        b"FILE \"20261007_021434.BIN\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00\nINDEX 01 00:00:00\n",
        b"FILE \"20261007_021434.BIN\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00\nTRACK 01 AUDIO\nINDEX 01 00:00:20\n",
        b"FILE \"20261007_021434.BIN\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:01:00\n",
    ] {
        fs::write(&cue,invalid).unwrap();
        assert!(container::source_info(&cue).is_err());
    }
}

#[test]
fn metadata_uses_later_data_track_start() {
    let dir=tempfile::tempdir().unwrap();
    let cue=dir.path().join("20261007_021434.CUE");
    let bin=dir.path().join("20261007_021434.BIN");
    let mut image=vec![0u8;2352*200];
    let base=150;
    let mut pvd=[0u8;2048];
    pvd[0]=1; pvd[1..6].copy_from_slice(b"CD001"); pvd[6]=1;
    pvd[40..50].copy_from_slice(b"MIXED_DISC");
    pvd[156]=34;
    pvd[158..162].copy_from_slice(&20u32.to_le_bytes());
    pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
    write_sector(&mut image,base+16,&pvd);
    let mut root=[0u8;2048];
    let name=b"SYSTEM.CNF;1";
    root[0]=46; root[2..6].copy_from_slice(&21u32.to_le_bytes());
    root[10..14].copy_from_slice(&32u32.to_le_bytes());
    root[32]=name.len() as u8; root[33..33+name.len()].copy_from_slice(name);
    write_sector(&mut image,base+20,&root);
    let mut cnf=[0u8;2048];
    let boot=b"BOOT = cdrom:\\SLES_012.34;1\r\n";
    cnf[..boot.len()].copy_from_slice(boot);
    write_sector(&mut image,base+21,&cnf);
    fs::write(&bin,&image).unwrap();
    fs::write(&cue,b"FILE \"20261007_021434.BIN\" BINARY\r\n  TRACK 01 AUDIO\r\n    INDEX 01 00:00:00\r\n  TRACK 02 MODE2/2352\r\n    INDEX 01 00:02:00\r\n").unwrap();

    let info=container::source_info(&cue).unwrap();
    assert_eq!(info.disc.serial.as_deref(),Some("SLES-01234"));
    assert_eq!(info.disc.region.as_deref(),Some("Europe"));
    assert_eq!((info.track_count,info.data_track_count),(2,1));
    let px=dir.path().join("mixed.px1");
    let cancel=AtomicBool::new(false);
    let report=compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    assert_eq!(report.manifest.disc.serial.as_deref(),Some("SLES-01234"));
    let out=dir.path().join("out"); fs::create_dir(&out).unwrap();
    extract(&px,&out,&cancel,|_|{}).unwrap();
    assert_eq!(fs::read(out.join("20261007_021434.BIN")).unwrap(),image);
    assert_eq!(fs::read(out.join("20261007_021434.CUE")).unwrap(),fs::read(&cue).unwrap());
}

#[test]
fn progress_covers_hashing_and_container_checks() {
    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    let px=dir.path().join("progress.px1");
    let cancel=AtomicBool::new(false);
    let events=RefCell::new(Vec::new());
    compress(&cue,&px,1,None,None,&cancel,|p| events.borrow_mut().push(p)).unwrap();
    for stage in ["Hashing source","Sealing container","Checking container"] {
        let last=events.borrow().iter().filter(|p|p.stage==stage).last().cloned().unwrap();
        assert!(last.total>0,"{stage}");
        assert_eq!(last.done,last.total,"{stage}");
    }

    let cancelled=dir.path().join("cancel-hash.px1");
    let cancel=AtomicBool::new(false);
    let error=compress(&cue,&cancelled,1,None,None,&cancel,|p| {
        if p.stage=="Hashing source" && p.done>0 { cancel.store(true,Ordering::SeqCst); }
    }).unwrap_err();
    assert!(error.contains("cancelled"));
    assert!(!cancelled.exists());
    assert!(!dir.path().join("cancel-hash.px1.tmp").exists());
}

#[test]
fn oversized_source_cue_is_rejected_before_import() {
    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    fs::OpenOptions::new().write(true).open(&cue).unwrap().set_len(8*1024*1024+1).unwrap();
    assert!(container::source_info(&cue).unwrap_err().contains("CUE is too large"));
    let px=dir.path().join("oversized.px1");
    let cancel=AtomicBool::new(false);
    assert!(compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap_err().contains("CUE is too large"));
    assert!(!px.exists());
}

#[test]
fn manifest_cannot_extract_to_windows_device_name() {
    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    let px=dir.path().join("valid.px1");
    let cancel=AtomicBool::new(false);
    compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    let mut bytes=fs::read(&px).unwrap();
    let original=b"20261007_021434.BIN";
    let device=b"CON.123456789012345";
    assert_eq!(original.len(),device.len());
    let at=bytes.windows(original.len()).position(|part|part==original).unwrap();
    bytes[at..at+original.len()].copy_from_slice(device);
    let footer_at=bytes.len()-32;
    let digest=Sha256::digest(&bytes[..footer_at]);
    bytes[footer_at..].copy_from_slice(&digest);
    let unsafe_px=dir.path().join("unsafe.px1");
    fs::write(&unsafe_px,bytes).unwrap();
    assert!(inspect(&unsafe_px).unwrap_err().contains("Invalid manifest fields"));
    assert!(extract(&unsafe_px,dir.path(),&cancel,|_|{}).is_err());
    assert!(!dir.path().join("CON.123456789012345").exists());
}

#[test]
fn invalid_utf8_file_label_cannot_select_replacement_named_bin() {
    let dir=tempfile::tempdir().unwrap();
    let cue=dir.path().join("invalid-name.cue");
    fs::write(dir.path().join("bad�.bin"),vec![0u8;2352]).unwrap();
    fs::write(&cue,b"FILE \"bad\xff.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n").unwrap();
    let error=container::source_info(&cue).unwrap_err();
    assert!(error.contains("Unsupported CUE filename encoding"),"{error}");

    let plain_bin=dir.path().join("plain.bin");
    fs::write(&plain_bin,vec![0u8;2352]).unwrap();
    let non_path_bytes=b"FILE \"plain.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\nREM title \xff\r\n";
    fs::write(&cue,non_path_bytes).unwrap();
    let px=dir.path().join("non-path-encoding.px1");
    let cancel=AtomicBool::new(false);
    compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    let out=dir.path().join("out"); fs::create_dir(&out).unwrap();
    extract(&px,&out,&cancel,|_|{}).unwrap();
    assert_eq!(fs::read(out.join("invalid-name.cue")).unwrap(),non_path_bytes);
}

#[test]
fn utf8_bom_cue_roundtrips_without_rewriting() {
    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    let mut bytes=b"\xEF\xBB\xBFFILE \"20261007_021434.BIN\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n".to_vec();
    bytes.extend_from_slice(b"REM Preserved exactly\r\n");
    fs::write(&cue,&bytes).unwrap();
    let px=dir.path().join("bom.px1");
    let cancel=AtomicBool::new(false);
    compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    let out=dir.path().join("out"); fs::create_dir(&out).unwrap();
    extract(&px,&out,&cancel,|_|{}).unwrap();
    assert_eq!(fs::read(out.join("20261007_021434.CUE")).unwrap(),bytes);
}

#[test]
#[ignore = "full-size synthetic disc; run explicitly"]
fn full_size_disc_streams_and_roundtrips() {
    fn digest(path: &std::path::Path) -> [u8;32] {
        let mut file=fs::File::open(path).unwrap();
        let mut hash=Sha256::new();
        let mut buffer=[0u8;1024*1024];
        loop {
            let read=file.read(&mut buffer).unwrap();
            if read==0 { break; }
            hash.update(&buffer[..read]);
        }
        hash.finalize().into()
    }

    let dir=tempfile::tempdir().unwrap();
    let cue=source(dir.path());
    let bin=dir.path().join("20261007_021434.BIN");
    let size=(700u64*1024*1024/2352)*2352;
    fs::OpenOptions::new().write(true).open(&bin).unwrap().set_len(size).unwrap();
    let px=dir.path().join("full-size.px1");
    let cancel=AtomicBool::new(false);
    let compressed=compress(&cue,&px,1,None,None,&cancel,|_|{}).unwrap();
    assert!(compressed.chunks_verified>100);
    let out=dir.path().join("out"); fs::create_dir(&out).unwrap();
    let extracted=extract(&px,&out,&cancel,|_|{}).unwrap();
    assert!(extracted.bin_verified && extracted.cue_verified);
    assert_eq!(fs::metadata(&bin).unwrap().len(),size);
    assert_eq!(fs::metadata(out.join("20261007_021434.BIN")).unwrap().len(),size);
    assert_eq!(digest(&bin),digest(&out.join("20261007_021434.BIN")));
    assert_eq!(digest(&cue),digest(&out.join("20261007_021434.CUE")));
}
