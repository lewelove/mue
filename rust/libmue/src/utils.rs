use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use sha1::{Digest, Sha1};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fmt::Write;

pub struct SourceResolution {
    pub origin_path: String,
    pub is_in_store: bool,
    pub link_name: String,
    pub internal_name: String,
    pub sanitized_name: String,
    pub truncated_hash: String,
    pub album_name: String,
}

pub fn build_globset(tracks_filter: &str) -> Result<globset::GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for part in tracks_filter.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() { continue; }
        let pattern = if !trimmed.contains('/') && !trimmed.contains('*') && !trimmed.contains('?') {
            format!("**/*.{}", trimmed.trim_start_matches('.'))
        } else {
            trimmed.to_string()
        };
        builder.add(globset::Glob::new(&pattern)?);
    }
    Ok(builder.build()?)
}

pub fn detect_source_type(path: &Path, store: Option<&Path>) -> Result<String> {
    let path_str = path.to_string_lossy();
    let expr = format!(
        "let res = (import (/. + \"{path_str}\") {{ mue = {{ mkAlbum = x: x; }}; }}); in \
         if res ? source then \
             if res.source ? torrent then \"torrent\" \
             else if res.source ? web then \"web\" \
             else \"\" \
         else \"\""
    );
    log::debug!("Detecting source type for {}", path.display());
    let mut cmd = Command::new("nix");
    if let Some(s) = store {
        cmd.args(["--store", s.to_str().unwrap()]);
    }
    cmd.args(["eval", "--raw", "--impure", "--expr", &expr]);
    let output = cmd.output().context("Failed to execute nix eval for source type")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{err}");
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Detected source type: {result}");
    Ok(result)
}

pub fn resolve_source_origin(
    target_path: &Path,
    source_type: Option<&str>,
    store_path: &Path,
    origin_base_path: &Path,
) -> Result<SourceResolution> {
    let album_name = eval_nix_field::<std::collections::hash_map::RandomState>(target_path, "name", None, Some(store_path)).unwrap_or_default();
    
    let mut source_name_attr = String::new();
    let mut source_hash = String::new();
    let mut detected_torrent_name = String::new();
    
    if let Some(st) = source_type {
        source_name_attr = eval_nix_field::<std::collections::hash_map::RandomState>(target_path, &format!("source.{st}.name"), None, Some(store_path)).unwrap_or_default();
        source_hash = eval_nix_field::<std::collections::hash_map::RandomState>(target_path, &format!("source.{st}.hash"), None, Some(store_path)).unwrap_or_default();
        
        if st == "torrent" && source_name_attr.is_empty() {
            let torrent_file_raw = eval_nix_field::<std::collections::hash_map::RandomState>(target_path, "source.torrent.file", None, Some(store_path)).unwrap_or_default();
            if !torrent_file_raw.is_empty() && torrent_file_raw != "null" {
                let torrent_path = if torrent_file_raw.starts_with("./") {
                    target_path.parent().unwrap_or_else(|| Path::new(".")).join(torrent_file_raw.trim_start_matches("./"))
                } else {
                    PathBuf::from(&torrent_file_raw)
                };
                if torrent_path.exists()
                    && let Ok(torrent) = lava_torrent::torrent::v1::Torrent::read_from_file(&torrent_path)
                {
                    detected_torrent_name = torrent.name;
                }
            }
        }
    }

    let internal_name = if !source_name_attr.is_empty() {
        source_name_attr
    } else if !detected_torrent_name.is_empty() {
        detected_torrent_name
    } else {
        album_name.clone()
    };

    log::debug!("Resolved internal source name: {internal_name}");

    let truncated = get_nix32_truncate(&source_hash, Some(store_path));
    let sanitized = sanitize_source_name(&internal_name);
    let link_name = format!("{sanitized}-{truncated}");

    let gc_roots_source = store_path.join("gcroots").join("source");
    let source_link = gc_roots_source.join(&link_name);
    
    let mut is_in_store = false;
    let mut store_origin_path = String::new();

    if fs::symlink_metadata(&source_link).is_ok()
        && let Ok(logical_target) = fs::read_link(&source_link)
    {
        let logical_str = logical_target.to_string_lossy().to_string();
        if logical_str.starts_with("/nix/store/") {
            let physical_target = store_path.join("nix/store").join(logical_str.trim_start_matches("/nix/store/"));
            if physical_target.exists() {
                is_in_store = true;
                store_origin_path = physical_target.to_string_lossy().to_string();
            }
        }
    }

    let actual_origin_path = if is_in_store {
        store_origin_path
    } else if source_type == Some("torrent") {
        origin_base_path.join("torrent").join(&link_name).to_string_lossy().to_string()
    } else {
        let origin_path_nix = eval_nix_field::<std::collections::hash_map::RandomState>(target_path, "origin.path", None, Some(store_path)).unwrap_or_default();
        if !origin_path_nix.is_empty() {
            PathBuf::from(origin_path_nix).to_string_lossy().to_string()
        } else {
            origin_base_path.join(&album_name).to_string_lossy().to_string()
        }
    };

    Ok(SourceResolution {
        origin_path: actual_origin_path,
        is_in_store,
        link_name,
        internal_name,
        sanitized_name: sanitized,
        truncated_hash: truncated,
        album_name,
    })
}

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
    log::debug!("File hash result: {hash}");
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
    log::debug!("Path (NAR) hash result: {hash}");
    Ok(hash)
}

pub fn check_hash(actual: &str, expected: &str, name: &str) -> Result<()> {
    log::debug!("Validating {name} - Expected: {expected} | Actual: {actual}");
    if expected.is_empty() || expected == "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" {
        anyhow::bail!("{name} hash is missing or uses a placeholder.\nActual hash: {actual}");
    }
    if actual != expected {
        anyhow::bail!("{name} hash mismatch.\nExpected: {expected}\nActual:   {actual}");
    }
    Ok(())
}

#[must_use] 
pub fn get_mue_flake_uri() -> String {
    let home = dirs::home_dir().expect("Could not find home directory");
    let config_dir = home.join(".config/mue");
    format!("path:{}", config_dir.display())
}

#[must_use] 
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
        log::debug!("Truncated base32 hash: {truncated}");
        return truncated;
    }
    let fallback = hash.trim_start_matches("sha256-").chars().take(32).collect::<String>().replace('/', "_").replace('+', "-");
    log::debug!("Base32 conversion failed, using fallback: {fallback}");
    fallback
}

#[must_use] 
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

pub fn eval_nix_field<S: std::hash::BuildHasher>(path: &Path, field_path: &str, envs: Option<&HashMap<String, String, S>>, store: Option<&Path>) -> Result<String> {
    let path_str = path.to_string_lossy();
    let expr = format!(
        "let res = (import (/. + \"{path_str}\") {{ mue = {{ mkAlbum = x: x; }}; }}); in builtins.toString (res.{field_path} or \"\")"
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
        anyhow::bail!("{err}");
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Evaluated field '{field_path}': {result}");
    Ok(result)
}

pub fn eval_nix_derivation_field<S: std::hash::BuildHasher>(path: &Path, field_path: &str, envs: Option<&HashMap<String, String, S>>, store: Option<&Path>) -> Result<String> {
    let path_str = path.to_string_lossy();
    let flake_uri = get_mue_flake_uri();
    let expr = format!(
        "(import (/. + \"{path_str}\") {{ mue = (builtins.getFlake \"{flake_uri}\").lib; }}).{field_path}"
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
        anyhow::bail!("{err}");
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Evaluated derivation field '{field_path}': {result}");
    Ok(result)
}

pub fn eval_config_field<S: std::hash::BuildHasher>(path: &Path, field_path: &str, envs: Option<&HashMap<String, String, S>>, store: Option<&Path>) -> Result<String> {
    let flake_uri = get_mue_flake_uri();
    let path_str = path.to_string_lossy();
    let expr = format!(
        "let mue = (builtins.getFlake \"{flake_uri}\").lib; \
             args = import (/. + \"{path_str}\") {{ mue = mue // {{ mkAlbum = x: x; }}; }}; \
             config = mue.evalConfig args; \
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
        anyhow::bail!("{err}");
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log::debug!("Evaluated config field '{field_path}': {result}");
    Ok(result)
}

pub fn resolve_album_path(path: &str) -> Result<PathBuf> {
    log::debug!("Resolving album path for input: {path}");
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

#[must_use] 
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

#[must_use] 
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

pub fn resolve_ctdbtocid(folder_path: &Path, tracks_filter: Option<&str>) -> Option<String> {
    log::debug!("Scanning directory for files: {}", folder_path.display());
    if !folder_path.exists() {
        log::error!("Source directory does not exist: {}", folder_path.display());
        return None;
    }

    let globset = tracks_filter.and_then(|f| build_globset(f).ok());

    let all_files: Vec<PathBuf> = walkdir::WalkDir::new(folder_path)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path().to_path_buf())
        .filter(|p| p.is_file())
        .collect();

    log::debug!("Found {} total files in the directory tree.", all_files.len());

    let mut files: Vec<PathBuf> = all_files.into_iter()
        .filter(|p| {
            if let Some(gs) = &globset {
                if let Ok(rel) = p.strip_prefix(folder_path) {
                    if gs.is_match(rel) {
                        return true;
                    }
                    let mut components = rel.components();
                    if components.next().is_some() {
                        let sub = components.as_path();
                        if !sub.as_os_str().is_empty() && gs.is_match(sub) {
                            return true;
                        }
                    }
                    false
                } else {
                    gs.is_match(p)
                }
            } else {
                true
            }
        })
        .collect();

    log::debug!("{} files matched the tracks filter.", files.len());

    if files.is_empty() {
        log::error!("No files available to generate CTDB TOCID.");
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
