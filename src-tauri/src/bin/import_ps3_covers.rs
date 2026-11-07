use image::{imageops, ImageReader};
use std::{env, fs, io::Cursor, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return Err("usage: import_ps3_covers <extracted-pack-folder> <artwork-root>".into());
    }
    let input = Path::new(&args[1]);
    let root = Path::new(&args[2]);
    let mut converted = 0u64;
    let mut skipped = 0u64;
    let mut failed = Vec::new();
    let mut source_bytes = 0u64;
    let mut output_bytes = 0u64;
    fn visit(dir: &Path, root: &Path, converted: &mut u64, skipped: &mut u64, failed: &mut Vec<String>, source_bytes: &mut u64, output_bytes: &mut u64) -> Result<(), Box<dyn std::error::Error>> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() { visit(&path, root, converted, skipped, failed, source_bytes, output_bytes)?; continue; }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else { continue; };
            if !path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("jpg") || s.eq_ignore_ascii_case("png")) { continue; }
            if id.len() != 9 || !id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) { failed.push(path.display().to_string()); continue; }
            let full = root.join("ps3/library/covers").join(format!("{id}.webp"));
            let thumb = root.join("thumbnails/ps3/library/covers").join(format!("{id}.webp"));
            if full.exists() && thumb.exists() { *skipped += 1; continue; }
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let bytes = fs::read(&path)?;
                if bytes.len() > 12 * 1024 * 1024 { return Err("source exceeds 12 MiB".into()); }
                let reader = ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
                let (w, h) = reader.into_dimensions()?;
                if w == 0 || h == 0 || w > 4096 || h > 4096 { return Err("invalid dimensions".into()); }
                let rgba = image::load_from_memory(&bytes)?.to_rgba8();
                let resized = imageops::thumbnail(&rgba, w.min(160), h.min(160));
                let full_bytes = webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(85.0).to_vec();
                let thumb_bytes = webp::Encoder::from_rgba(resized.as_raw(), resized.width(), resized.height()).encode(85.0).to_vec();
                fs::create_dir_all(full.parent().ok_or("invalid destination")?)?;
                fs::create_dir_all(thumb.parent().ok_or("invalid destination")?)?;
                fs::write(&full, &full_bytes)?;
                fs::write(&thumb, &thumb_bytes)?;
                *source_bytes += bytes.len() as u64;
                *output_bytes += (full_bytes.len() + thumb_bytes.len()) as u64;
                Ok(())
            })();
            match result { Ok(()) => *converted += 1, Err(e) => failed.push(format!("{}: {e}", path.display())) }
        }
        Ok(())
    }
    visit(input, root, &mut converted, &mut skipped, &mut failed, &mut source_bytes, &mut output_bytes)?;
    println!("converted={converted} skipped={skipped} failed={} source_bytes={source_bytes} output_bytes={output_bytes}", failed.len());
    for item in failed { eprintln!("FAILED {item}"); }
    Ok(())
}
