use anyhow::Result;
use crate::utils::{eval_nix_field, eval_nix_derivation_field, eval_config_field, resolve_album_path, expand_path, ground_logical_path};
use crate::config::AppConfig;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::HashMap;

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

pub fn run(path: &str, source_type: Option<&str>) -> Result<()> {
    log::debug!("Starting munix build for path: {}", path);

    let config = AppConfig::load();
    let store_path = config.get_store_path();
    log::debug!("Using Nix store at: {}", store_path.display());

    sync_env(&store_path)?;

    let target_path = resolve_album_path(path)?;
    let target_dir = target_path.parent().unwrap();

    if source_type == Some("torrent") {
        log::debug!("Source type is torrent, initiating pre-verify sequence");
        let torrent_file_raw = eval_nix_field(&target_path, "source.torrent.file", None, Some(&store_path))?;
        let torrent_hash = eval_nix_field(&target_path, "source.torrent.hash", None, Some(&store_path))?;
        
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
            }
        }
    }

    let origin_base_path = expand_path(config.origin.as_deref().unwrap_or("."));
    let origin_base = origin_base_path.to_string_lossy().to_string();
    log::debug!("Mapping MUNIX_ORIGIN_PATH: {}", origin_base);

    let res = crate::utils::resolve_source_origin(
        &target_path,
        source_type,
        &store_path,
        &origin_base_path,
    )?;

    let mut envs = HashMap::new();
    envs.insert("MUNIX_ORIGIN_PATH".to_string(), res.origin_path.clone());
    log::debug!("Mapping MUNIX_SOURCE_NAME: {}", res.internal_name);
    envs.insert("MUNIX_SOURCE_NAME".to_string(), res.internal_name.clone());
    envs.insert("MUNIX_SANITIZED_SOURCE_NAME".to_string(), res.sanitized_name.clone());

    if source_type == Some("torrent") {
        if res.is_in_store {
            log::info!("Found pinned source in store. Skipping verification...");
        } else {
            let verify_cmd_raw = eval_config_field(&target_path, "commands.torrent.verify", Some(&envs), Some(&store_path))?;
            if !verify_cmd_raw.is_empty() {
                let verify_cmd = ground_logical_path(verify_cmd_raw, &store_path);
                log::info!("Executing torrent verification command");
                log::debug!("Verify command: {}", verify_cmd);
                let status = Command::new("sh").envs(&envs).arg("-c").arg(&verify_cmd).status()?;
                if !status.success() {
                    anyhow::bail!("Torrent verification failed. Logic returned non-zero exit code.");
                }
            }

            let physical_origin = PathBuf::from(&res.origin_path);
            if physical_origin.exists() {
                let actual_origin_hash = crate::utils::get_path_hash(&physical_origin, Some(&store_path))?;
                let origin_hash = eval_nix_field(&target_path, "origin.hash", None, Some(&store_path))?;
                log::debug!("Comparing NAR hashes for origin content");
                crate::utils::check_hash(&actual_origin_hash, &origin_hash, "origin.hash")?;
            } else {
                anyhow::bail!("Origin path does not exist: {}", physical_origin.display());
            }
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

    let base_expr = format!("(import ./album.nix {{ munix = (builtins.getFlake \"{}\").lib; }})", crate::utils::get_munix_flake_uri());
    
    let mut build_formats = Vec::new();
    if config.library.as_ref().and_then(|l| l.flac.as_ref()).and_then(|f| f.enable).unwrap_or(false) {
        build_formats.push("flac");
    }
    if config.library.as_ref().and_then(|l| l.opus.as_ref()).and_then(|o| o.enable).unwrap_or(false) {
        build_formats.push("opus");
    }

    let mut format_store_paths = HashMap::new();

    for fmt in build_formats {
        let fmt_expr = format!("{}.{}", base_expr, fmt);
        let result_link = store_path.join("gcroots").join("albums").join(format!("{}-{}-{}", res.album_name, res.truncated_hash, fmt));
        fs::create_dir_all(result_link.parent().unwrap())?;

        log::info!("Building {} derivation via Nix...", fmt);
        log::debug!("Nix command expression: {}", fmt_expr);
        log::debug!("Nix result link: {}", result_link.display());

        let mut build_cmd = Command::new("nix");
        build_cmd.envs(&envs);
        build_cmd.arg("build")
            .arg("--store")
            .arg(&store_path)
            .arg("--impure")
            .arg("--expr")
            .arg(&fmt_expr)
            .arg("--out-link")
            .arg(&result_link)
            .current_dir(target_dir);

        let status = build_cmd.status()?;
        if !status.success() {
            anyhow::bail!("Nix build failed for format: {}", fmt);
        }

        let logical_path = fs::read_link(&result_link)?;
        log::debug!("Build success for {}. Logical store path: {}", fmt, logical_path.display());

        let physical_store_path = store_path.join(logical_path.strip_prefix("/").unwrap_or(&logical_path));
        log::debug!("Materializing symlinks from physical store: {}", physical_store_path.display());
        
        materialize_output(&physical_store_path, target_dir, &store_path, &config, fmt)?;
        format_store_paths.insert(fmt.to_string(), physical_store_path);
    }

    sync_library(target_dir, &config, &format_store_paths, &store_path)?;

    if let Ok(src_logical_path) = eval_nix_derivation_field(&target_path, "sourceStorePath", Some(&envs), Some(&store_path)) {
        log::info!("Pinning source logical GC root...");
        let gc_roots_source = store_path.join("gcroots").join("source");
        fs::create_dir_all(&gc_roots_source)?;
        let source_link = gc_roots_source.join(&res.link_name);

        if source_link.exists() || source_link.symlink_metadata().is_ok() {
            let _ = fs::remove_file(&source_link);
        }

        if let Err(e) = std::os::unix::fs::symlink(&src_logical_path, &source_link) {
            log::warn!("Failed to create source GC root link: {}", e);
        }

        envs.insert("MUNIX_ORIGIN_PATH".to_string(), src_logical_path);
        
        let seed_cmd_raw = eval_config_field(&target_path, "commands.torrent.seed", Some(&envs), Some(&store_path)).unwrap_or_default();
        if !seed_cmd_raw.is_empty() {
            let seed_cmd = ground_logical_path(seed_cmd_raw, &store_path);
            log::info!("Executing seed lifecycle command");
            log::debug!("Seed command: {}", seed_cmd);
            let _ = Command::new("sh").envs(&envs).arg("-c").arg(&seed_cmd).status();
        }
    }

    log::info!("Build completed successfully.");
    Ok(())
}

fn materialize_output(store_dir: &Path, target_dir: &Path, store_path: &Path, config: &AppConfig, current_fmt: &str) -> Result<()> {
    log::debug!("Cleaning up target directory for {}: {}", current_fmt, target_dir.display());
    
    let link_allowed = match current_fmt {
        "flac" => config.library.as_ref().and_then(|l| l.flac.as_ref()).and_then(|f| f.link_to_album_root).unwrap_or(true),
        "opus" => config.library.as_ref().and_then(|l| l.opus.as_ref()).and_then(|o| o.link_to_album_root).unwrap_or(false),
        _ => true,
    };

    if let Ok(entries) = fs::read_dir(target_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Ok(meta) = fs::symlink_metadata(&path)
                && meta.is_symlink()
                && let Ok(target) = fs::read_link(&path)
                && target.starts_with(store_path)
            {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                let is_metadata = path.file_name().and_then(|s| s.to_str()) == Some("metadata.toml");
                
                if ext == current_fmt || (current_fmt == "flac" && is_metadata) {
                    log::debug!("Removing old {} symlink: {}", current_fmt, path.display());
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    if !link_allowed {
        log::debug!("Materialization skipped for format {} based on link_to_album_root config.", current_fmt);
        return Ok(());
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

fn sync_library(target_dir: &Path, config: &AppConfig, format_store_paths: &HashMap<String, PathBuf>, store_path: &Path) -> Result<()> {
    let toml_path = target_dir.join("metadata.toml");
    if !toml_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&toml_path)?;
    let parsed: toml::Value = toml::from_str(&content)?;
    
    let albumartist_raw = parsed.get("album").and_then(|a| a.get("albumartist")).and_then(|a| a.as_str()).unwrap_or("");
    let album_raw = parsed.get("album").and_then(|a| a.get("album")).and_then(|a| a.as_str()).unwrap_or("Unknown Album");
    
    let albumartist = if albumartist_raw.is_empty() {
        "Unknown Artist"
    } else {
        albumartist_raw
    };

    let folder_name = if albumartist == "Unknown Artist" {
        album_raw.replace("/", "_")
    } else {
        format!("{} - {}", albumartist, album_raw).replace("/", "_")
    };

    if let Some(lib) = &config.library {
        if let Some(flac_cfg) = &lib.flac
            && flac_cfg.enable.unwrap_or(false)
            && flac_cfg.link_to_library_root.unwrap_or(false)
            && let Some(root) = &flac_cfg.root
            && !root.is_empty()
            && let Some(store_dir) = format_store_paths.get("flac")
        {
            log::debug!("Syncing flac collection for: {}", folder_name);
            link_format_to_library(store_dir, root, &folder_name, "flac", store_path)?;
        }
        if let Some(opus_cfg) = &lib.opus 
            && opus_cfg.enable.unwrap_or(false)
            && opus_cfg.link_to_library_root.unwrap_or(false)
            && let Some(root) = &opus_cfg.root
            && !root.is_empty()
            && let Some(store_dir) = format_store_paths.get("opus")
        {
            log::debug!("Syncing opus collection for: {}", folder_name);
            link_format_to_library(store_dir, root, &folder_name, "opus", store_path)?;
        }
    }
    Ok(())
}

fn link_format_to_library(store_dir: &Path, root: &str, folder_name: &str, ext_filter: &str, store_path: &Path) -> Result<()> {
    let expanded_root = crate::utils::expand_path(root);
    if !expanded_root.exists() {
        fs::create_dir_all(&expanded_root)?;
    }
    let lib_album_dir = expanded_root.join(folder_name);
    if !lib_album_dir.exists() {
        fs::create_dir_all(&lib_album_dir)?;
    } else if let Ok(entries) = fs::read_dir(&lib_album_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Some(ext) = path.extension() 
                && (ext == ext_filter || ext == "jpg" || ext == "png" || ext == "toml") 
            {
                log::debug!("Cleaning up library file: {}", path.display());
                let _ = fs::remove_file(&path);
            }
        }
    }

    log::debug!("Scanning store path for library links: {}", store_dir.display());
    if let Ok(entries) = fs::read_dir(store_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let file_name = path.file_name().unwrap();
            let dest = lib_album_dir.join(file_name);
            let mut source = path.clone();

            if let Ok(target) = fs::read_link(&path) {
                if target.is_absolute() {
                    source = PathBuf::from(ground_logical_path(target.to_string_lossy().to_string(), store_path));
                } else {
                    source = store_dir.join(target);
                }
            }

            log::debug!("Creating library link: {} -> {}", dest.display(), source.display());
            let _ = std::os::unix::fs::symlink(&source, &dest);
        }
    }
    Ok(())
}
