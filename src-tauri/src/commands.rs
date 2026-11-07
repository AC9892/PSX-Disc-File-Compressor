use px1_core::{self as px, container::{source_info_with_bin, compress_with_bin_strategy, compress_with_bin_combined}, DiscInfo, Manifest, Report};
use std::{fs::OpenOptions, io::Write, path::PathBuf, sync::{Arc,atomic::{AtomicBool,Ordering}}};
use tauri::{Emitter, State};
use crate::ps2::{self, DiscSource, ChdReport};
use crate::ps3::{self, Ps3Info, Ps3Report};
use crate::artwork::{self, ArtworkStats};
use tauri::Manager;

#[tauri::command]
pub fn artwork_default_dir(app: tauri::AppHandle) -> Result<String,String> {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(collection) = artwork::nearby_collection(&executable) {
            return Ok(collection.display().to_string());
        }
    }
    Ok(app.path().app_local_data_dir().map_err(|e|e.to_string())?.join("artwork").display().to_string())
}
#[tauri::command]
pub fn artwork_lookup(root: String, platform: String, variant: String, serial: String, thumbnail: bool, manual_only: Option<bool>) -> Result<Option<String>,String> { artwork::lookup_with_source(std::path::Path::new(&root),&platform,&variant,&serial,thumbnail,manual_only.unwrap_or(false)) }
#[tauri::command]
pub async fn artwork_stats(root: String) -> Result<ArtworkStats,String> { tauri::async_runtime::spawn_blocking(move || artwork::stats(std::path::Path::new(&root))).await.map_err(|e|e.to_string())? }
#[tauri::command]
pub fn artwork_replace(root: String, platform: String, variant: String, serial: String, source: String) -> Result<(),String> { artwork::replace(std::path::Path::new(&root),&platform,&variant,&serial,std::path::Path::new(&source)) }
#[tauri::command]
pub fn artwork_remove(root: String, platform: String, variant: String, serial: String) -> Result<(),String> { artwork::remove(std::path::Path::new(&root),&platform,&variant,&serial) }
#[tauri::command]
pub async fn artwork_clear_cache(root: String) -> Result<u64,String> { tauri::async_runtime::spawn_blocking(move || artwork::clear_cache(std::path::Path::new(&root))).await.map_err(|e|e.to_string())? }
#[tauri::command]
pub async fn artwork_download_ps3(root: String, title_id: String) -> Result<Option<String>,String> { tauri::async_runtime::spawn_blocking(move || artwork::download_ps3(std::path::Path::new(&root),&title_id)).await.map_err(|e|e.to_string())? }

#[tauri::command]
pub async fn inspect_ps3(path: String) -> Result<Ps3Info,String> { tauri::async_runtime::spawn_blocking(move || ps3::inspect(&PathBuf::from(path))).await.map_err(|e|e.to_string())? }
#[tauri::command]
pub async fn resolve_ps3_cue(path: String) -> Result<Option<String>,String> {
    tauri::async_runtime::spawn_blocking(move || ps3::cue_iso_reference(&PathBuf::from(path)).map(|p| p.map(|p| p.display().to_string()))).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn inspect_ps3z(path: String) -> Result<Ps3Info,String> { tauri::async_runtime::spawn_blocking(move || ps3::inspect_archive(&PathBuf::from(path))).await.map_err(|e|e.to_string())? }
#[tauri::command]
pub async fn compress_ps3(state: State<'_,JobState>, path: String, output_path: String) -> Result<Ps3Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || { let _guard=guard; ps3::compress(&PathBuf::from(path),&PathBuf::from(output_path),&cancel) }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn verify_ps3(state: State<'_,JobState>, path: String) -> Result<Ps3Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || { let _guard=guard; ps3::verify(&PathBuf::from(path),&cancel) }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn extract_ps3(state: State<'_,JobState>, path: String, output_dir: String) -> Result<Ps3Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || { let _guard=guard; ps3::extract(&PathBuf::from(path),&PathBuf::from(output_dir),&cancel) }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub async fn inspect_ps2(path: String, bin_path: Option<String>) -> Result<DiscSource,String> {
    tauri::async_runtime::spawn_blocking(move || ps2::inspect(&PathBuf::from(path), bin_path.as_deref().map(std::path::Path::new))).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub async fn compress_ps2(state: State<'_,JobState>, source: DiscSource, output_path: String) -> Result<ChdReport,String> {
    let (guard,_) = start(&state)?;
    tauri::async_runtime::spawn_blocking(move || { let _guard=guard; ps2::compress(&source,&PathBuf::from(output_path)) }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub async fn verify_ps2(state: State<'_,JobState>, path: String) -> Result<ChdReport,String> {
    let (guard,_) = start(&state)?;
    tauri::async_runtime::spawn_blocking(move || { let _guard=guard; ps2::verify(&PathBuf::from(path)) }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub async fn extract_ps2(state: State<'_,JobState>, path: String, output_dir: String) -> Result<ChdReport,String> {
    let (guard,_) = start(&state)?;
    tauri::async_runtime::spawn_blocking(move || { let _guard=guard; ps2::extract(&PathBuf::from(path),&PathBuf::from(output_dir)) }).await.map_err(|e|e.to_string())?
}

#[derive(Default)]
pub struct JobState { busy: Arc<AtomicBool>, cancel: Arc<AtomicBool> }

struct BusyGuard(Arc<AtomicBool>);
impl Drop for BusyGuard { fn drop(&mut self) { self.0.store(false,Ordering::SeqCst); } }

fn start(state: &JobState) -> Result<(BusyGuard,Arc<AtomicBool>),String> {
    state.busy.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).map_err(|_|"Another operation is already running")?;
    state.cancel.store(false,Ordering::SeqCst);
    Ok((BusyGuard(state.busy.clone()),state.cancel.clone()))
}

#[tauri::command]
pub async fn inspect_cue(path: String, bin_path: Option<String>) -> Result<DiscInfo,String> {
    tauri::async_runtime::spawn_blocking(move || {
        let selected = bin_path.map(PathBuf::from);
        source_info_with_bin(&PathBuf::from(path), selected.as_deref())
    }).await.map_err(|e|e.to_string())?
}

#[derive(serde::Serialize)]
pub struct Px1Info { manifest: Manifest, px1_size: u64, container_sha256: String }

#[tauri::command]
pub async fn inspect_px1(path: String) -> Result<Px1Info,String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (manifest,px1_size,container_sha256)=px::inspect(&PathBuf::from(path))?;
        Ok(Px1Info{manifest,px1_size,container_sha256})
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub fn cancel_job(state: State<'_,JobState>) { state.cancel.store(true,Ordering::SeqCst); }

#[tauri::command]
pub fn save_log(path: String, content: String) -> Result<(),String> {
    let target=PathBuf::from(path);
    let mut file=OpenOptions::new().write(true).create_new(true).open(target).map_err(|e|format!("Cannot create log: {e}"))?;
    file.write_all(content.as_bytes()).map_err(|e|format!("Cannot save log: {e}"))
}

#[tauri::command]
pub async fn compress_disc(app: tauri::AppHandle, state: State<'_,JobState>, cue_path: String, bin_path: Option<String>, output_path: String, level: i32, best_size: bool, combined: bool, title: Option<String>, disc_number: Option<u32>) -> Result<Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard=guard;
        let selected = bin_path.map(PathBuf::from);
        if combined {
            compress_with_bin_combined(&PathBuf::from(cue_path),selected.as_deref(),&PathBuf::from(output_path),title,disc_number,&cancel,|p| { let _=app.emit("px1-progress",p); })
        } else {
            compress_with_bin_strategy(&PathBuf::from(cue_path),selected.as_deref(),&PathBuf::from(output_path),level,best_size,title,disc_number,&cancel,|p| { let _=app.emit("px1-progress",p); })
        }
    }).await.map_err(|e|format!("Worker failed: {e}"))?
}

#[tauri::command]
pub async fn extract_disc(app: tauri::AppHandle, state: State<'_,JobState>, path: String, output_dir: String) -> Result<Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard=guard;
        px::extract(&PathBuf::from(path),&PathBuf::from(output_dir),&cancel,|p| { let _=app.emit("px1-progress",p); })
    }).await.map_err(|e|format!("Worker failed: {e}"))?
}

#[tauri::command]
pub async fn verify_disc(app: tauri::AppHandle, state: State<'_,JobState>, path: String) -> Result<Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard=guard;
        px::verify(&PathBuf::from(path),&cancel,|p| { let _=app.emit("px1-progress",p); })
    }).await.map_err(|e|format!("Worker failed: {e}"))?
}
