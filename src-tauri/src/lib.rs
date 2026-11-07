pub mod commands;
pub mod ps2;
pub mod ps3;
pub mod artwork;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::JobState::default())
        .invoke_handler(tauri::generate_handler![commands::inspect_cue, commands::inspect_px1, commands::compress_disc, commands::extract_disc, commands::verify_disc, commands::inspect_ps2, commands::compress_ps2, commands::verify_ps2, commands::extract_ps2, commands::inspect_ps3, commands::resolve_ps3_cue, commands::inspect_ps3z, commands::compress_ps3, commands::verify_ps3, commands::extract_ps3, commands::artwork_default_dir, commands::artwork_lookup, commands::artwork_stats, commands::artwork_replace, commands::artwork_remove, commands::artwork_clear_cache, commands::artwork_download_ps3, commands::cancel_job, commands::save_log])
        .run(tauri::generate_context!())
        .expect("error while running PX1");
}
