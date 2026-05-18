use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use sha1::{Digest, Sha1};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fmt::Write;

pub fn get_file_hash(path: &Path, store: Option<&Path>) -> Result<String> {
    log::debug!("Calculating file hash for: {}", path.display());
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    cmd.args(["hash", "file", path.to_str().unwrap()]);
    let output = cmd.output().context("Failed to execute nix hash file")?;
    if !output.status.success() {
        anyhow::bail!("Failed to calculate file hash for {}", path.display());
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("File hash result: {}", hash);
    Ok(hash)
}

pub fn get_path_hash(path: &Path, store: Option<&Path>) -> Result<String> {
    log::debug!("Calculating path (NAR) hash for: {}", path.display());
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    cmd.args(["hash", "path", path.to_str().unwrap()]);
    let output = cmd.output().context("Failed to execute nix hash path")?;
    if !output.status.success() {
        anyhow::bail!("Failed to calculate path (NAR) hash for {}", path.display());
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Path (NAR) hash result: {}", hash);
    Ok(hash)
}

pub fn check_hash(actual: &str, expected: &str, name: &str) -> Result<()> {
    log::debug!("Validating {name} - Expected: {expected} | Actual: {actual}");
    if expected.is_empty() || expected == "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" {
        anyhow::bail!("{} hash is missing or uses a placeholder.\nActual hash: {}", name, actual);
    }
    if actual != expected {
        anyhow::bail!("{} hash mismatch.\nExpected: {}\nActual:   {}", name, expected, actual);
    }
    Ok(())
}

pub fn get_munix_flake_uri() -> String {
    let home = dirs::home_dir().expect("Could not find home directory");
    let config_dir = home.join(".config/munix");
    format!("path:{}", config_dir.display())
}

pub fn get_nix32_truncate(hash: &str, store: Option<&Path>) -> String {
    if hash.is_empty() {
        return "nohash".to_string();
    }
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    cmd.args(["hash", "to-base32", hash]);
    let output = cmd.output();
    if let Ok(out) = output && out.status.success() {
        let nix32 = String::from_utf8(out.stdout).unwrap_or_default().trim().to_string();
        let truncated: String = nix32.chars().take(32).collect();
        log::debug!("Truncated base32 hash: {}", truncated);
        return truncated;
    }
    let fallback = hash.trim_start_matches("sha256-").chars().take(32).collect::<String>().replace('/', "_").replace('+', "-");
    log::debug!("Base32 conversion failed, using fallback: {}", fallback);
    fallback
}

pub fn sanitize_source_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn ground_logical_path(input: String, store_path: &Path) -> String {
    let logical_prefix = "/nix/store/";
    let mut physical_prefix = store_path.to_path_buf();
    physical_prefix.push("nix/store/");
    let physical_prefix_str = physical_prefix.to_string_lossy().to_string();
    input.replace(logical_prefix, &physical_prefix_str)
}

pub fn eval_nix_field(path: &Path, field_path: &str, envs: Option<&HashMap<String, String>>, store: Option<&Path>) -> Result<String> {
    let path_str = path.to_string_lossy();
    let expr = format!(
        "let res = (import (/. + \"{path_str}\") {{ munix = {{ mkAlbum = x: x; }}; }}); in builtins.toString (res.{field_path} or \"\")"
    );
    log::debug!("Evaluating nix field '{}' from {}", field_path, path.display());
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    if let Some(e) = envs {
        cmd.envs(e);
    }
    cmd.args(["eval", "--raw", "--impure", "--expr", &expr]);
    let output = cmd.output().context("Failed to execute nix eval")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", err);
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Evaluated field '{}': {}", field_path, result);
    Ok(result)
}

pub fn eval_nix_derivation_field(path: &Path, field_path: &str, envs: Option<&HashMap<String, String>>, store: Option<&Path>) -> Result<String> {
    let path_str = path.to_string_lossy();
    let flake_uri = get_munix_flake_uri();
    let expr = format!(
        "(import (/. + \"{path_str}\") {{ munix = (builtins.getFlake \"{flake_uri}\").lib; }}).{field_path}"
    );
    log::debug!("Evaluating real derivation field '{}' from {}", field_path, path.display());
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    if let Some(e) = envs {
        cmd.envs(e);
    }
    cmd.args(["eval", "--raw", "--impure", "--expr", &expr]);
    let output = cmd.output().context("Failed to execute nix eval for derivation")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", err);
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Evaluated derivation field '{}': {}", field_path, result);
    Ok(result)
}

pub fn eval_config_field(path: &Path, field_path: &str, envs: Option<&HashMap<String, String>>, store: Option<&Path>) -> Result<String> {
    let flake_uri = get_munix_flake_uri();
    let path_str = path.to_string_lossy();
    let expr = format!(
        "let munix = (builtins.getFlake \"{flake_uri}\").lib; \
             args = import (/. + \"{path_str}\") {{ munix = munix // {{ mkAlbum = x: x; }}; }}; \
             config = munix.evalConfig args; \
         in builtins.toString (config.{field_path} or \"\")"
    );
    log::debug!("Evaluating config field '{}' for album {}", field_path, path.display());
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    if let Some(e) = envs {
        cmd.envs(e);
    }
    cmd.args(["eval", "--raw", "--impure", "--expr", &expr]);
    let output = cmd.output().context("Failed to execute nix eval for config")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", err);
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Evaluated config field '{}': {}", field_path, result);
    Ok(result)
}

pub fn resolve_album_path(path: &str) -> Result<PathBuf> {
    log::debug!("Resolving album path for input: {}", path);
    let p = Path::new(path).canonicalize().context("Path does not exist")?;
    if p.is_dir() && p.join("album.nix").exists() {
        log::debug!("Found album.nix inside directory: {}", p.display());
        Ok(p.join("album.nix"))
    } else if p.is_file() && p.file_name().unwrap_or_default() == "album.nix" {
        log::debug!("Input path points directly to album.nix: {}", p.display());
        Ok(p)
    } else {
        anyhow::bail!("No album.nix found at specified path");
    }
}

pub fn expand_path(path_str: &str) -> PathBuf {
    if path_str.starts_with('~')
        && let Some(home) = dirs::home_dir()
    {
        if path_str == "~" {
            return home;
        }
        if let Some(stripped) = path_str.strip_prefix("~/") {
            return home.join(stripped);
        }
    }
    PathBuf::from(path_str)
}

pub fn get_sort_key(filepath: &Path) -> (u8, u32, String) {
    if let Ok(tag) = metaflac::Tag::read_from_path(filepath)
        && let Some(vc) = tag.vorbis_comments()
        && let Some(track_nums) = vc.get("TRACKNUMBER")
        && let Some(num_str) = track_nums.first()
    {
        let num_part = num_str.split('/').next().unwrap_or("0");
        if let Ok(n) = num_part.parse::<u32>() {
            return (0, n, String::new());
        }
    }
    let filename = filepath.file_name().unwrap_or_default().to_string_lossy().to_string();
    (1, 0, filename)
}

pub fn resolve_ctdbtocid(folder_path: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder_path)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("flac"))
        .collect();

    if files.is_empty() {
        return None;
    }

    files.sort_by(|a, b| {
        let key_a = get_sort_key(a);
        let key_b = get_sort_key(b);
        
        match key_a.0.cmp(&key_b.0) {
            std::cmp::Ordering::Equal => {
                match key_a.1.cmp(&key_b.1) {
                    std::cmp::Ordering::Equal => alphanumeric_sort::compare_path(a, b),
                    other => other,
                }
            },
            other => other,
        }
    });

    let mut offsets = Vec::new();
    let mut current_offset = 150;

    for f in &files {
        if let Ok(tag) = metaflac::Tag::read_from_path(f)
            && let Some(info) = tag.get_streaminfo()
        {
            let sectors = info.total_samples / 588;
            offsets.push(current_offset);
            current_offset += sectors;
        }
    }
    
    if offsets.is_empty() {
        return None;
    }

    let leadout = current_offset;
    let pregap = offsets[0];

    let mut x = String::new();
    for offset in offsets.iter().skip(1) {
        let _ = write!(x, "{:08X}", offset - pregap);
    }
    let _ = write!(x, "{:08X}", leadout - pregap);

    while x.len() < 800 {
        x.push('0');
    }

    let mut hasher = Sha1::new();
    hasher.update(x.as_bytes());
    let sha1_bytes = hasher.finalize();

    let ctdb = STANDARD
        .encode(sha1_bytes)
        .replace('+', ".")
        .replace('/', "_")
        .replace('=', "-");

    Some(ctdb)
}
