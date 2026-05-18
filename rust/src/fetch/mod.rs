use anyhow::Result;
use crate::utils::{eval_config_field, eval_nix_field, resolve_album_path};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::process::Command;

pub fn run(source: &str, path: &str) -> Result<()> {
    log::debug!("Starting fetch operation");
    log::debug!("Source: {}", source);
    log::debug!("Target path: {}", path);

    let config = AppConfig::load();
    let store_path = config.get_store_path();
    let album_path = resolve_album_path(path)?;

    if source == "torrent" {
        let torrent_hash = eval_nix_field(&album_path, "source.torrent.hash", None, Some(&store_path))?;
        let source_name = eval_nix_field(&album_path, "source.torrent.name", None, Some(&store_path))?;
        let truncated = crate::utils::get_nix32_truncate(&torrent_hash, Some(&store_path));
        let sanitized = crate::utils::sanitize_source_name(&source_name);
        let link_name = format!("{sanitized}-{truncated}");
        
        let source_link = store_path.join("gcroots").join("source").join(&link_name);
        if source_link.exists() {
            log::info!("Source is already pinned logically in the Nix store. Skipping fetch.");
            return Ok(());
        }
    }

    let mut envs = HashMap::new();

    if let Some(config_origin) = &config.origin {
        let expanded = crate::utils::expand_path(config_origin).to_string_lossy().to_string();
        log::debug!("Using origin path from config: {}", expanded);
        envs.insert("MUNIX_ORIGIN_PATH".to_string(), expanded);
    }

    let fallback_name = eval_nix_field(&album_path, "name", None, Some(&store_path)).unwrap_or_default();
    log::debug!("Using fallback source name: {}", fallback_name);
    envs.insert("MUNIX_SOURCE_NAME".to_string(), fallback_name);

    let cmd_field = match source {
        "torrent" => "commands.torrent.fetch",
        "web" => "commands.web.fetch",
        _ => anyhow::bail!("Unknown source: '{}'. Expected 'torrent' or 'web'.", source),
    };

    let final_cmd = eval_config_field(&album_path, cmd_field, Some(&envs), Some(&store_path))?;

    if final_cmd.is_empty() {
        anyhow::bail!("Command '{}' is missing or empty in config.nix", cmd_field);
    }

    log::debug!("Final resolved fetch command: {}", final_cmd);

    log::info!("Executing fetch command...");
    let mut cmd = Command::new("sh");
    cmd.envs(&envs).arg("-c").arg(&final_cmd);
    let status = cmd.status()?;
    
    if !status.success() {
        anyhow::bail!("Command failed with status: {}", status);
    }

    log::info!("Fetch operation completed successfully");

    Ok(())
}
