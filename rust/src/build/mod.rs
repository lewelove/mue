use anyhow::Result;
use crate::utils::{eval_nix_field, eval_nix_derivation_field, eval_config_field, resolve_album_path, expand_path, get_nix32_truncate, sanitize_source_name};
use crate::config::AppConfig;
use lava_torrent::torrent::v1::Torrent;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::HashMap;

pub fn run(path: &str, source_type: Option<&str>) -> Result<()> {
    log::debug!("Starting munix build for path: {}", path);

    let config = AppConfig::load();
    let store_path = config.get_store_path();
    log::debug!("Using Nix store at: {}", store_path.display());

    sync_env(&store_path)?;

    let target_path = resolve_album_path(path)?;
    let target_dir = target_path.parent().unwrap();

    let mut detected_torrent_name = String::new();
    let mut torrent_hash = String::new();

    if source_type == Some("torrent") {
        log::debug!("Source type is torrent, initiating pre-verify sequence");
        let torrent_file_raw = eval_nix_field(&target_path, "source.torrent.file", None, Some(&store_path))?;
        torrent_hash = eval_nix_field(&target_path, "source.torrent.hash", None, Some(&store_path))?;
        
        if !torrent_file_raw.is_empty() {
            let torrent_path = if torrent_file_raw.starts_with("./") {
                target_dir.join(torrent_file_raw.trim_start_matches("./"))
            } else {
                PathBuf::from(&torrent_file_raw)
            };
            log::debug!("Resolved torrent file path: {}", torrent_path.display());

            if torrent_path.exists() {
                let actual_torrent_hash = crate::utils::get_file_hash(&torrent_path, Some(&store_path))?;
                crate::utils::check_hash(&actual_torrent_hash, &torrent_hash, "source.torrent.hash")?;
                
                log::debug!("Parsing torrent metadata via lava_torrent");
                let torrent = Torrent::read_from_file(&torrent_path)
                    .map_err(|e| anyhow::anyhow!("Failed to parse torrent file: {}", e))?;
                
                detected_torrent_name = torrent.name;
                log::debug!("Detected torrent name: {}", detected_torrent_name);
            }
        }
    }

    let origin_base_path = expand_path(config.origin.as_deref().unwrap_or("."));
    let origin_base = origin_base_path.to_string_lossy().to_string();
    log::debug!("Mapping MUNIX_ORIGIN_PATH: {}", origin_base);

    let album_name = eval_nix_field(&target_path, "name", None, Some(&store_path))?;
    let mut source_name_attr = String::new();
    if let Some(st) = source_type {
        source_name_attr = eval_nix_field(&target_path, &format!("source.{}.name", st), None, Some(&store_path))?;
    }

    let internal_torrent_name = if !source_name_attr.is_empty() {
        source_name_attr
    } else if !detected_torrent_name.is_empty() {
        detected_torrent_name
    } else {
        album_name.clone()
    };

    let truncated = get_nix32_truncate(&torrent_hash, Some(&store_path));
    let sanitized = sanitize_source_name(&internal_torrent_name);
    let link_name = format!("{sanitized}-{truncated}");

    let mut envs = HashMap::new();
    let actual_origin_path = if source_type == Some("torrent") {
        let p = origin_base_path.join("torrent").join(&link_name);
        log::debug!("Resolved origin path via base + torrent/ + link_name: {}", p.display());
        p
    } else {
        let origin_path_nix = eval_nix_field(&target_path, "origin.path", None, Some(&store_path))?;
        if !origin_path_nix.is_empty() {
            log::debug!("Using explicit origin.path: {}", origin_path_nix);
            PathBuf::from(origin_path_nix)
        } else {
            origin_base_path.join(&album_name)
        }
    };

    envs.insert("MUNIX_ORIGIN_PATH".to_string(), actual_origin_path.to_string_lossy().to_string());
    log::debug!("Mapping MUNIX_SOURCE_NAME: {}", internal_torrent_name);
    envs.insert("MUNIX_SOURCE_NAME".to_string(), internal_torrent_name);

    if source_type == Some("torrent") {
        let verify_cmd = eval_config_field(&target_path, "commands.torrent.verify", Some(&envs), Some(&store_path))?;
        if !verify_cmd.is_empty() {
            log::info!("Executing torrent verification command");
            log::debug!("Verify command: {}", verify_cmd);
            let status = Command::new("sh").envs(&envs).arg("-c").arg(&verify_cmd).status()?;
            if !status.success() {
                anyhow::bail!("Torrent verification failed. Logic returned non-zero exit code.");
            }
        }

        if actual_origin_path.exists() {
            let actual_origin_hash = crate::utils::get_path_hash(&actual_origin_path, Some(&store_path))?;
            let origin_hash = eval_nix_field(&target_path, "origin.hash", None, Some(&store_path))?;
            log::debug!("Comparing NAR hashes for origin content");
            crate::utils::check_hash(&actual_origin_hash, &origin_hash, "origin.hash")?;
        } else {
            anyhow::bail!("Origin path does not exist: {}", actual_origin_path.display());
        }
    }

    let cover_file_raw = eval_nix_field(&target_path, "cover.file", None, Some(&store_path))?;
    let cover_hash = eval_nix_field(&target_path, "cover.hash", None, Some(&store_path))?;
    if !cover_file_raw.is_empty() && cover_file_raw != "null" {
        let cover_path = if cover_file_raw.starts_with("./") {
            target_dir.join(cover_file_raw.trim_start_matches("./"))
        } else {
            PathBuf::from(&cover_file_raw)
        };
        log::debug!("Validating cover file: {}", cover_path.display());
        if cover_path.exists() {
            let actual_cover_hash = crate::utils::get_file_hash(&cover_path, Some(&store_path))?;
            crate::utils::check_hash(&actual_cover_hash, &cover_hash, "cover.hash")?;
        } else {
            anyhow::bail!("Cover file not found at {}", cover_path.display());
        }
    }

    let expr = format!("(import ./album.nix {{ munix = (builtins.getFlake \"{}\").lib; }})", crate::utils::get_munix_flake_uri());
    let result_link = store_path.join("gcroots").join("albums").join(format!("{}-{}", album_name, truncated));
    fs::create_dir_all(result_link.parent().unwrap())?;

    log::info!("Building package via Nix...");
    log::debug!("Nix command expression: {}", expr);
    log::debug!("Nix result link: {}", result_link.display());

    let mut build_cmd = Command::new("nix");
    build_cmd.envs(&envs);
    build_cmd.arg("build")
        .arg("--store")
        .arg(&store_path)
        .arg("--impure")
        .arg("--expr")
        .arg(&expr)
        .arg("--out-link")
        .arg(&result_link)
        .current_dir(target_dir);

    let status = build_cmd.status()?;
    if !status.success() {
        anyhow::bail!("Nix build failed.");
    }

    let logical_path = fs::read_link(&result_link)?;
    log::debug!("Build success. Logical store path: {}", logical_path.display());

    let physical_store_path = store_path.join(logical_path.strip_prefix("/").unwrap_or(&logical_path));
    log::debug!("Materializing symlinks from physical store: {}", physical_store_path.display());
    
    materialize_output(&physical_store_path, target_dir, &store_path)?;

    if let Ok(src_store_path) = eval_nix_derivation_field(&target_path, "sourceStorePath", Some(&envs), Some(&store_path)) {
        log::info!("Pinning source logical GC root...");
        let gc_roots_source = store_path.join("gcroots").join("source");
        fs::create_dir_all(&gc_roots_source)?;
        let source_link = gc_roots_source.join(&link_name);

        if source_link.exists() || source_link.symlink_metadata().is_ok() {
            let _ = fs::remove_file(&source_link);
        }

        if let Err(e) = std::os::unix::fs::symlink(&src_store_path, &source_link) {
            log::warn!("Failed to create source GC root link: {}", e);
        }

        envs.insert("MUNIX_ORIGIN_PATH".to_string(), src_store_path);
        let seed_cmd = eval_config_field(&target_path, "commands.torrent.seed", Some(&envs), Some(&store_path)).unwrap_or_default();
        if !seed_cmd.is_empty() {
            log::info!("Executing seed lifecycle command");
            let _ = Command::new("sh").envs(&envs).arg("-c").arg(&seed_cmd).status();
        }
    }

    log::info!("Build completed successfully.");
    Ok(())
}

fn sync_env(store_path: &Path) -> Result<()> {
    let flake_uri = crate::utils::get_munix_flake_uri();
    let gc_roots_profiles = store_path.join("gcroots").join("profiles");
    fs::create_dir_all(&gc_roots_profiles)?;
    let active_env_link = gc_roots_profiles.join("env");

    log::info!("Syncing toolchain environment GC root...");
    let mut cmd = Command::new("nix");
    cmd.arg("build")
        .arg("--store")
        .arg(store_path)
        .arg("--impure")
        .arg(format!("{flake_uri}#env"))
        .arg("--out-link")
        .arg(&active_env_link);

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("Nix build failed for env toolchain sync");
    }
    Ok(())
}

fn materialize_output(store_dir: &Path, target_dir: &Path, store_path: &Path) -> Result<()> {
    log::debug!("Cleaning up target directory: {}", target_dir.display());
    if let Ok(entries) = fs::read_dir(target_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Ok(meta) = fs::symlink_metadata(&path)
                && meta.is_symlink()
                && let Ok(target) = fs::read_link(&path)
                && target.starts_with(store_path)
            {
                log::debug!("Removing old build symlink: {}", path.display());
                let _ = fs::remove_file(&path);
            }
        }
    }

    for entry in fs::read_dir(store_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        if file_name == "album.nix" { continue; }

        let mut store_file = entry.path();
        let target_file = target_dir.join(&file_name);

        if let Ok(meta) = fs::symlink_metadata(&target_file) {
            if meta.is_dir() { 
                log::debug!("Replacing existing directory: {}", target_file.display());
                fs::remove_dir_all(&target_file)?; 
            } else { 
                log::debug!("Replacing existing file: {}", target_file.display());
                fs::remove_file(&target_file)?; 
            }
        }

        if let Ok(resolved_path) = fs::read_link(&store_file) {
            if resolved_path.starts_with("/nix/store") {
                store_file = store_path.join(resolved_path.strip_prefix("/").unwrap());
            } else if resolved_path.is_relative() {
                store_file = store_dir.join(resolved_path);
            } else {
                store_file = resolved_path;
            }
        } else if store_file.starts_with("/nix/store") {
            store_file = store_path.join(store_file.strip_prefix("/").unwrap());
        }

        log::debug!("Creating link: {} -> {}", target_file.display(), store_file.display());
        std::os::unix::fs::symlink(&store_file, &target_file)?;
    }
    Ok(())
}
