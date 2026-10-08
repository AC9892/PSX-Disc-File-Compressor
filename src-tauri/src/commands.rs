use px1_core::{self as px, container::{source_info_with_bin, compress_with_bin_strategy}, DiscInfo, Manifest, Report};
use std::{fs::OpenOptions, io::Write, path::PathBuf, sync::{Arc,atomic::{AtomicBool,Ordering}}};
use tauri::{Emitter, State};

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
pub fn inspect_cue(path: String, bin_path: Option<String>) -> Result<DiscInfo,String> {
    let selected = bin_path.map(PathBuf::from);
    source_info_with_bin(&PathBuf::from(path), selected.as_deref())
}

#[derive(serde::Serialize)]
pub struct Px1Info { manifest: Manifest, px1_size: u64, container_sha256: String }

#[tauri::command]
pub fn inspect_px1(path: String) -> Result<Px1Info,String> {
    let (manifest,px1_size,container_sha256)=px::inspect(&PathBuf::from(path))?;
    Ok(Px1Info{manifest,px1_size,container_sha256})
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
pub async fn compress_disc(app: tauri::AppHandle, state: State<'_,JobState>, cue_path: String, bin_path: Option<String>, output_path: String, level: i32, best_size: bool, title: Option<String>, disc_number: Option<u32>) -> Result<Report,String> {
    let (guard,cancel)=start(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard=guard;
        let selected = bin_path.map(PathBuf::from);
        compress_with_bin_strategy(&PathBuf::from(cue_path),selected.as_deref(),&PathBuf::from(output_path),level,best_size,title,disc_number,&cancel,|p| { let _=app.emit("px1-progress",p); })
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
