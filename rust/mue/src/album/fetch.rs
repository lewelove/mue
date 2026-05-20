use anyhow::Result;
use libmue::utils::{eval_config_field, resolve_album_path, expand_path};
use libmue::config::AppConfig;
use std::collections::HashMap;
use std::process::Command;

pub fn run(path: &str) -> Result<()> {
    log::debug!("Starting fetch operation");
    log::debug!("Target path: {path}");

    let config = AppConfig::load();
    let store_path = config.get_store_path();
    let album_path = resolve_album_path(path)?;
    
    let source_type_str = libmue::utils::detect_source_type(&album_path, Some(&store_path))?;
    if source_type_str.is_empty() {
        anyhow::bail!("No valid source found in album.nix");
    }
    let source = source_type_str.as_str();

    let origin_base_path = expand_path(config.origin.as_deref().unwrap_or("."));

    let res = libmue::utils::resolve_source_origin(
        &album_path,
        Some(source),
        &store_path,
        &origin_base_path,
    )?;

    if source == "torrent" && res.is_in_store {
        log::info!("Source is already pinned logically in the Nix store. Skipping fetch.");
        return Ok(());
    }

    let mut envs = HashMap::new();
    log::debug!("Using origin path: {}", res.origin_path);
    envs.insert("MUE_ORIGIN_PATH".to_string(), res.origin_path);
    envs.insert("MUE_SOURCE_NAME".to_string(), res.internal_name);
    envs.insert("MUE_SANITIZED_SOURCE_NAME".to_string(), res.sanitized_name);

    let cmd_field = match source {
        "torrent" => "commands.torrent.fetch",
        "web" => "commands.web.fetch",
        _ => anyhow::bail!("Unknown source: '{source}'. Expected 'torrent' or 'web'."),
    };

    let final_cmd = eval_config_field(&album_path, cmd_field, Some(&envs), Some(&store_path))?;

    if final_cmd.is_empty() {
        anyhow::bail!("Command '{cmd_field}' is missing or empty in config.nix");
    }

    log::debug!("Final resolved fetch command: {final_cmd}");
    
    let env_bin = store_path.join("gcroots/profiles/env/bin");
    let injected_path = format!("{}:{}", env_bin.display(), std::env::var("PATH").unwrap_or_default());

    log::info!("Executing fetch command...");
    let mut cmd = Command::new("sh");
    cmd.env("PATH", injected_path).envs(&envs).arg("-c").arg(&final_cmd);
    let status = cmd.status()?;
    
    if !status.success() {
        anyhow::bail!("Command failed with status: {status}");
    }

    log::info!("Fetch operation completed successfully");

    Ok(())
}
