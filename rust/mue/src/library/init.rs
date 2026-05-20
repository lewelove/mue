use anyhow::Result;
use std::fs;

pub fn run(path: &str) -> Result<()> {
    let target = libmue::utils::expand_path(path);
    fs::create_dir_all(&target)?;
    
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&target)
        .status();
        
    Ok(())
}
