use libmue::config::AppConfig;
use libmue::utils::{resolve_album_path, resolve_source_origin, expand_path};
use std::path::PathBuf;

pub fn run(source: Option<&str>, tracks: Option<&str>) {
    let mut origin_folder = PathBuf::from(".");

    if let Ok(target_path) = resolve_album_path(".")
        && source.is_some()
    {
        let config = AppConfig::load();
        let store_path = config.get_store_path();
        let origin_base_path = expand_path(config.origin.as_deref().unwrap_or("."));

        if let Ok(res) = resolve_source_origin(&target_path, source, &store_path, &origin_base_path) {
            origin_folder = PathBuf::from(res.origin_path);
        }
    }

    if let Some(ctdb) = libmue::utils::resolve_ctdbtocid(&origin_folder, tracks) {
        println!("https://db.cuetools.net/?tocid={ctdb}");
    } else {
        std::process::exit(1);
    }
}
