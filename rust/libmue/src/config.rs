use serde::Deserialize;
use std::process::Command;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct OpusConfig {
    pub enable: Option<bool>,
    #[allow(dead_code)]
    pub kbps: Option<u32>,
    pub root: Option<String>,
    pub link_to_album_root: Option<bool>,
    pub link_to_library_root: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct FlacConfig {
    pub enable: Option<bool>,
    pub root: Option<String>,
    pub link_to_album_root: Option<bool>,
    pub link_to_library_root: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct LibraryConfig {
    pub flac: Option<FlacConfig>,
    pub opus: Option<OpusConfig>,
}

#[derive(Deserialize, Default)]
pub struct AppConfig {
    pub store: Option<String>,
    pub origin: Option<String>,
    pub environment: Option<String>,
    pub library: Option<LibraryConfig>,
}

impl AppConfig {
    #[must_use] 
    pub fn load() -> Self {
        let flake_uri = crate::utils::get_mue_flake_uri();
        let expr = format!("builtins.toJSON ((builtins.getFlake \"{flake_uri}\").lib.evalConfig {{}})");
        
        let output = Command::new("nix")
            .args(["eval", "--raw", "--impure", "--expr", &expr])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let json_str = String::from_utf8_lossy(&out.stdout);
                serde_json::from_str(&json_str).unwrap_or_default()
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                log::error!("Config evaluation error:\n{err}");
                std::process::exit(1);
            }
            Err(e) => {
                log::error!("Failed to execute nix eval for config: {e}");
                std::process::exit(1);
            }
        }
    }

    #[must_use] 
    pub fn get_store_path(&self) -> PathBuf {
        let store_dir = self.store.as_deref().unwrap_or("/nix/store");
        crate::utils::expand_path(store_dir)
            .canonicalize()
            .unwrap_or_else(|_| crate::utils::expand_path(store_dir))
    }
}
