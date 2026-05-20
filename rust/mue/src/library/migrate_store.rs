use anyhow::Result;
use libmue::config::AppConfig;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn run() -> Result<()> {
    let config = AppConfig::load();
    let store_path = config.get_store_path();
    
    let mut target_dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        target_dirs.push(cwd);
    }
    
    if let Some(lib) = config.library {
        if let Some(flac) = lib.flac
            && let Some(root) = flac.root {
            target_dirs.push(PathBuf::from(root));
        }
        if let Some(opus) = lib.opus
            && let Some(root) = opus.root {
            target_dirs.push(PathBuf::from(root));
        }
    }

    for dir in target_dirs {
        let expanded = libmue::utils::expand_path(&dir.to_string_lossy());
        if !expanded.exists() { continue; }
        
        for entry in WalkDir::new(&expanded).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if fs::symlink_metadata(path).map(|m| m.is_symlink()).unwrap_or(false)
                && let Ok(target) = fs::read_link(path) {
                let target_str = target.to_string_lossy();
                if let Some(idx) = target_str.find("/nix/store/") {
                    let suffix = &target_str[idx + 11..];
                    let physical_target = store_path.join("nix/store").join(suffix);
                    fs::remove_file(path)?;
                    std::os::unix::fs::symlink(physical_target, path)?;
                }
            }
        }
    }
    Ok(())
}
