use anyhow::Result;
use walkdir::WalkDir;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut albums = Vec::new();
    
    for entry in WalkDir::new(&cwd).into_iter().filter_map(Result::ok) {
        if entry.file_name() == "album.nix"
            && let Some(parent) = entry.path().parent() {
            albums.push(parent.to_path_buf());
        }
    }
    
    for album_dir in albums {
        let path_str = album_dir.to_string_lossy().to_string();
        let _ = crate::album::build::run(&path_str, None);
    }
    
    crate::library::migrate_store::run()?;
    Ok(())
}
