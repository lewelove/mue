use anyhow::Result;
use lava_torrent::torrent::v1::Torrent;
use std::path::{Path, PathBuf};
use std::fmt::Write;
use toml::Value;

pub fn run(path_str: &str, tracks_filter: &str, torrent_path: Option<&str>, metadata_path: Option<&str>) -> Result<()> {
    let target_dir = Path::new(path_str).canonicalize().unwrap_or_else(|_| PathBuf::from(path_str));
    
    let t_path = if let Some(t) = torrent_path {
        Path::new(t).to_path_buf()
    } else {
        let mut found = None;
        if target_dir.is_dir() && let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("torrent") {
                    found = Some(entry.path());
                    break;
                }
            }
        }
        found.unwrap_or_else(|| PathBuf::from("."))
    };

    if !t_path.exists() {
        anyhow::bail!("Torrent file not found");
    }

    let torrent_hash = crate::utils::get_file_hash(&t_path, None).unwrap_or_default();
    let torrent = Torrent::read_from_file(&t_path).map_err(|_| anyhow::anyhow!("Torrent parse error"))?;

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
    let globset = builder.build()?;

    let mut valid_paths = Vec::new();
    if let Some(files) = &torrent.files {
        for f in files {
            if globset.is_match(f.path.to_string_lossy().as_ref()) {
                valid_paths.push(f.path.clone());
            }
        }
    }
    valid_paths.sort_by(|a, b| alphanumeric_sort::compare_path(a, b));

    let mut merged_meta = Value::Table(toml::map::Map::new());
    if let Some(m_path) = metadata_path && let p = Path::new(m_path) && p.exists() {
        let content = std::fs::read_to_string(p)?;
        let parsed: Value = toml::from_str(&content)?;
        deep_merge(&mut merged_meta, parsed);
    }

    let album_data = merged_meta.get("album");
    let artist = album_data.and_then(|a| a.get("albumartist")).and_then(Value::as_str).unwrap_or("");
    let album = album_data.and_then(|a| a.get("album")).and_then(Value::as_str).unwrap_or(&torrent.name);
    
    let pname_base = if artist.is_empty() {
        album.to_lowercase()
    } else {
        format!("{}-{}", artist.to_lowercase(), album.to_lowercase())
    };
    
    let sanitized_pname = pname_base.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>().split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");

    let mut out = String::new();
    let _ = writeln!(out, "{{ munix }}:");
    let _ = writeln!(out, "munix.mkAlbum {{");
    let _ = writeln!(out, "  name = \"{sanitized_pname}\";");
    let _ = writeln!(out, "  source.torrent = {{");
    let _ = writeln!(out, "    file = ./{};", t_path.file_name().unwrap_or_default().to_string_lossy());
    let _ = writeln!(out, "    name = \"\";");
    let _ = writeln!(out, "    hash = \"{torrent_hash}\";");
    let _ = writeln!(out, "  }};");
    let _ = writeln!(out, "  origin.hash = \"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\";");
    let _ = writeln!(out, "  cover = {{");
    let _ = writeln!(out, "    file = ./cover.png;");
    let _ = writeln!(out, "    hash = \"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\";");
    let _ = writeln!(out, "  }};");
    let _ = writeln!(out, "  album = {{");
    let _ = writeln!(out, "    metadata = {{");
    if let Some(data) = album_data {
        let _ = write!(out, "{}", to_nix_attributes(data, "      "));
    }
    let _ = writeln!(out, "    }};");
    let _ = writeln!(out, "  }};");
    let _ = writeln!(out, "  tracks = [");

    let meta_tracks = merged_meta.get("tracks").and_then(Value::as_array);
    let meta_tracks_len = meta_tracks.map_or(0, Vec::len);
    let total_count = std::cmp::max(valid_paths.len(), meta_tracks_len);

    for i in 0..total_count {
        let file_path = valid_paths.get(i).map_or_else(String::new, |path_buf| {
            if torrent.files.is_some() {
                format!("{}/{}", torrent.name, path_buf.to_string_lossy())
            } else {
                path_buf.to_string_lossy().to_string()
            }
        });

        let track_meta = if let Some(arr) = meta_tracks && i < arr.len() {
            arr[i].clone()
        } else {
            Value::Table(toml::map::Map::new())
        };

        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "      file = \"{file_path}\";");
        let _ = writeln!(out, "      metadata = {{");
        let _ = write!(out, "{}", to_nix_attributes(&track_meta, "        "));
        let _ = writeln!(out, "      }};");
        let _ = writeln!(out, "    }}");
    }

    let _ = writeln!(out, "  ];");
    let _ = writeln!(out, "}}");
    
    println!("{out}");
    Ok(())
}

fn to_nix_attributes(val: &Value, indent: &str) -> String {
    let mut res = String::new();
    if let Some(tab) = val.as_table() {
        for (k, v) in tab {
            let _ = writeln!(res, "{indent}{k} = {};", to_nix_value(v));
        }
    }
    res
}

fn to_nix_value(val: &Value) -> String {
    match val {
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(to_nix_value).collect();
            format!("[ {} ]", items.join(" "))
        }
        _ => "\"\"".to_string(),
    }
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Table(base_map), Value::Table(overlay_map)) => {
            for (k, v) in overlay_map {
                if k == "tracks" && v.is_array() && let Some(base_tracks) = base_map.get_mut("tracks") {
                    if let (Value::Array(b_arr), Value::Array(o_arr)) = (base_tracks, v) {
                        for (i, o_val) in o_arr.into_iter().enumerate() {
                            if i < b_arr.len() {
                                deep_merge(&mut b_arr[i], o_val);
                            } else {
                                b_arr.push(o_val);
                            }
                        }
                    }
                } else if let Some(base_v) = base_map.get_mut(&k) {
                    deep_merge(base_v, v);
                } else {
                    base_map.insert(k, v);
                }
            }
        }
        (base_val, overlay_val) => {
            *base_val = overlay_val;
        }
    }
}
