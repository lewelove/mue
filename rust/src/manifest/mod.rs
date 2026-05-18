use anyhow::Result;
use lava_torrent::torrent::v1::Torrent;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use serde_json::{json, Value};

pub fn run(path_str: &str, tracks_filter: &str, torrent_path: Option<&str>, metadata_path: Option<&str>, intermediary: bool) -> Result<()> {
    let target_dir = Path::new(path_str).canonicalize().unwrap_or_else(|_| PathBuf::from(path_str));
    
    let album_nix_path = target_dir.join("album.nix");
    let mut origin_hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string();
    if album_nix_path.exists()
        && let Ok(h) = crate::utils::eval_nix_field(&album_nix_path, "origin.hash", None, None)
        && !h.is_empty()
    {
        origin_hash = h;
    }

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

    let rel_t_path = t_path.canonicalize().unwrap_or(t_path.clone())
        .strip_prefix(&target_dir).map(|p| p.to_path_buf()).unwrap_or(t_path.clone());

    let mut cover_file = "cover.png".to_string();
    let mut cover_hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string();
    
    let p_png = target_dir.join("cover.png");
    let p_jpg = target_dir.join("cover.jpg");
    if p_png.exists() {
        if let Ok(h) = crate::utils::get_file_hash(&p_png, None) {
            cover_hash = h;
        }
    } else if p_jpg.exists() {
        cover_file = "cover.jpg".to_string();
        if let Ok(h) = crate::utils::get_file_hash(&p_jpg, None) {
            cover_hash = h;
        }
    }

    let globset = crate::utils::build_globset(tracks_filter)?;

    let mut valid_paths = Vec::new();
    if let Some(files) = &torrent.files {
        for f in files {
            if globset.is_match(f.path.to_string_lossy().as_ref()) {
                valid_paths.push(f.path.clone());
            }
        }
    }
    valid_paths.sort_by(|a, b| alphanumeric_sort::compare_path(a, b));

    let mut data = json!({
        "name": "",
        "origin": { "hash": origin_hash },
        "source": {
            "type": "torrent",
            "torrent": {
                "file": rel_t_path.to_string_lossy(),
                "name": torrent.name,
                "hash": torrent_hash
            },
            "web": { "url": "", "hash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" }
        },
        "cover": { "file": cover_file, "hash": cover_hash },
        "album": { 
            "info": {
                "total_discs": 1
            }, 
            "metadata": {}, 
            "mbid": {} 
        },
        "tracks": []
    });

    if let Some(m_path) = metadata_path {
        if m_path.starts_with("http") {
            let remote_data = crate::remote::fetch_musicbrainz_data(m_path)?;
            apply_remote_metadata(&mut data, remote_data);
        } else {
            let p = Path::new(m_path);
            if p.exists() {
                let content = std::fs::read_to_string(p)?;
                let parsed: toml::Value = toml::from_str(&content)?;
                let json_meta = serde_json::to_value(parsed)?;
                apply_local_metadata(&mut data, json_meta);
            }
        }
    }

    let album_name = data["album"]["metadata"]["album"].as_str().unwrap_or(&torrent.name);
    let artist_name = data["album"]["metadata"]["albumartist"].as_str().unwrap_or("");
    
    let pname_base = if artist_name.is_empty() {
        album_name.to_lowercase()
    } else {
        format!("{}-{}", artist_name.to_lowercase(), album_name.to_lowercase())
    };
    
    data["name"] = json!(pname_base.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>().split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-"));

    let mut tracks_list = Vec::new();
    let meta_tracks = data["tracks"].as_array().cloned().unwrap_or_default();

    for i in 0..std::cmp::max(valid_paths.len(), meta_tracks.len()) {
        let file_path = valid_paths.get(i).map_or_else(String::new, |path_buf| {
            if torrent.files.is_some() {
                format!("{}/{}", torrent.name, path_buf.to_string_lossy())
            } else {
                path_buf.to_string_lossy().to_string()
            }
        });

        let mut track_obj = meta_tracks.get(i).cloned().unwrap_or_else(|| json!({ "metadata": {}, "mbid": {} }));
        track_obj["file"] = json!(file_path);
        tracks_list.push(track_obj);
    }
    data["tracks"] = json!(tracks_list);

    sanitize_quotes(&mut data);

    if intermediary {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let json_str = serde_json::to_string(&data)?;
    let temp_path = target_dir.join(".munix-tmp.json");
    fs::write(&temp_path, &json_str)?;
    
    let temp_path_str = temp_path.to_string_lossy();
    let flake_uri = crate::utils::get_munix_flake_uri();
    let expr = format!("(builtins.getFlake \"{flake_uri}\").lib.albumNixTemplate {{ data = builtins.fromJSON (builtins.readFile (/. + \"{temp_path_str}\")); }}");
    
    let output = Command::new("nix").args(["eval", "--raw", "--impure", "--expr", &expr]).output();
    let _ = fs::remove_file(&temp_path);
    
    if let Ok(out) = output {
        if out.status.success() {
            println!("{}", String::from_utf8_lossy(&out.stdout).trim_end());
        } else {
            anyhow::bail!("Template generation failed:\n{}", String::from_utf8_lossy(&out.stderr));
        }
    }
    
    Ok(())
}

fn sanitize_quotes(v: &mut Value) {
    match v {
        Value::String(s) => *s = s.replace('’', "'"),
        Value::Array(arr) => {
            for item in arr {
                sanitize_quotes(item);
            }
        }
        Value::Object(map) => {
            for val in map.values_mut() {
                sanitize_quotes(val);
            }
        }
        _ => {}
    }
}

fn apply_local_metadata(data: &mut Value, json_meta: Value) {
    let mut max_disc = 1;
    if let Some(album_src) = json_meta.get("album").and_then(|v| v.as_object()) {
        for (k, v) in album_src {
            let target = if k.contains("musicbrainz") { "mbid" } else { "metadata" };
            data["album"][target][k] = v.clone();
        }
    }
    if let Some(tracks_src) = json_meta.get("tracks").and_then(|v| v.as_array()) {
        let mut tracks_out = Vec::new();
        for t in tracks_src {
            let mut t_obj = json!({ "metadata": {}, "mbid": {} });
            if let Some(t_map) = t.as_object() {
                for (k, v) in t_map {
                    if k == "discnumber" && let Some(d) = v.as_u64() && d > max_disc {
                        max_disc = d;
                    }
                    let target = if k.contains("musicbrainz") { "mbid" } else { "metadata" };
                    t_obj[target][k] = v.clone();
                }
            }
            tracks_out.push(t_obj);
        }
        data["tracks"] = json!(tracks_out);
    }
    data["album"]["info"]["total_discs"] = json!(max_disc);
}

fn apply_remote_metadata(data: &mut Value, remote: Value) {
    let rg = remote.get("release_group").unwrap();
    let rel = remote.get("release");
    let discogs = remote.get("discogs");

    let artist_obj = rg.get("artist-credit").and_then(|a| a.as_array()).and_then(|a| a.first());
    let artist = artist_obj.and_then(|c| c.get("name")).and_then(|n| n.as_str()).unwrap_or("Unknown Artist");
    let artist_id = artist_obj.and_then(|c| c.get("artist")).and_then(|a| a.get("id")).cloned().unwrap_or(Value::Null);

    let album = rg.get("title").and_then(|t| t.as_str()).unwrap_or("Unknown Album");
    let original_date = rg.get("first-release-date").and_then(|d| d.as_str()).unwrap_or("");

    data["album"]["metadata"]["albumartist"] = json!(artist);
    data["album"]["metadata"]["album"] = json!(album);
    data["album"]["metadata"]["original_date"] = json!(original_date);
    data["album"]["metadata"]["date"] = json!(if original_date.len() >= 4 { &original_date[..4] } else { original_date });
    data["album"]["mbid"]["musicbrainz_releasegroupid"] = rg.get("id").cloned().unwrap_or(Value::Null);
    data["album"]["mbid"]["musicbrainz_albumartistid"] = artist_id.clone();

    if let Some(dg) = discogs {
        let source = dg.get("master").or(dg.get("release"));
        if let Some(s) = source {
            if let Some(genres) = s.get("genres") { data["album"]["metadata"]["genre"] = genres.clone(); }
            if let Some(styles) = s.get("styles") { data["album"]["metadata"]["styles"] = styles.clone(); }
        }
    }

    if let Some(r) = rel {
        data["album"]["mbid"]["musicbrainz_albumid"] = r.get("id").cloned().unwrap_or(Value::Null);
        if let Some(country) = r.get("country") { data["album"]["metadata"]["country"] = country.clone(); }
        if let Some(release_date) = r.get("date") { data["album"]["metadata"]["release_date"] = release_date.clone(); }
        
        if let Some(label_info) = r.get("label-info").and_then(|l| l.as_array()).and_then(|l| l.first()) {
            if let Some(label_name) = label_info.get("label").and_then(|l| l.get("name")) {
                data["album"]["metadata"]["label"] = label_name.clone();
            }
            if let Some(cat_no) = label_info.get("catalog-number") {
                data["album"]["metadata"]["catalognumber"] = cat_no.clone();
            }
        }

        let mut tracks = Vec::new();
        if let Some(media) = r.get("media").and_then(|m| m.as_array()) {
            data["album"]["info"]["total_discs"] = json!(media.len());
            for (m_idx, medium) in media.iter().enumerate() {
                if let Some(track_list) = medium.get("tracks").and_then(|t| t.as_array()) {
                    for (t_idx, track) in track_list.iter().enumerate() {
                        let t_artist_obj = track.get("artist-credit").and_then(|a| a.as_array()).and_then(|a| a.first());
                        let t_artist = t_artist_obj.and_then(|c| c.get("name")).cloned().unwrap_or(json!(artist));
                        let t_artist_id = t_artist_obj.and_then(|c| c.get("artist")).and_then(|a| a.get("id")).cloned().unwrap_or(artist_id.clone());

                        tracks.push(json!({
                            "metadata": {
                                "tracknumber": t_idx + 1,
                                "discnumber": m_idx + 1,
                                "title": track.get("title").cloned().unwrap_or(json!("Untitled")),
                                "artist": t_artist
                            },
                            "mbid": {
                                "musicbrainz_trackid": track.get("recording").and_then(|r| r.get("id")).cloned().unwrap_or(Value::Null),
                                "musicbrainz_releasetrackid": track.get("id").cloned().unwrap_or(Value::Null),
                                "musicbrainz_artistid": t_artist_id
                            }
                        }));
                    }
                }
            }
        }
        data["tracks"] = json!(tracks);
    }
}
