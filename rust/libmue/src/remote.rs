use anyhow::{Context, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use crate::config::AppConfig;

fn get_discogs_token() -> Option<String> {
    let cfg = AppConfig::load();
    if let Some(env_path) = cfg.environment {
        let expanded = crate::utils::expand_path(&env_path);
        let _ = dotenvy::from_filename(expanded);
    }
    std::env::var("DISCOGS_API_TOKEN").ok()
}

pub fn fetch_musicbrainz_data(url: &str) -> Result<Value> {
    let re = Regex::new(r"(release|release-group)/([a-f0-9\-]+)").unwrap();
    let caps = re.captures(url).context("Invalid MusicBrainz URL")?;
    
    let mode = caps.get(1).unwrap().as_str();
    let entity_id = caps.get(2).unwrap().as_str();
    let is_rg = mode == "release-group";

    let cache_dir = dirs::home_dir()
        .map(|h| h.join(".cache/mue").join(mode))
        .context("Could not resolve cache directory")?;
    fs::create_dir_all(&cache_dir)?;

    let cache_file = cache_dir.join(format!("{entity_id}.json"));
    if cache_file.exists() {
        let content = fs::read_to_string(&cache_file)?;
        return Ok(serde_json::from_str(&content)?);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mue/0.1.0 ( https://github.com/lewelove/mue )")
        .build()?;

    let mut data = json!({ "_is_rg": is_rg });

    if is_rg {
        let rg_url = format!("https://musicbrainz.org/ws/2/release-group/{entity_id}?inc=tags+artist-credits+url-rels&fmt=json");
        let rg: Value = client.get(rg_url).send()?.json()?;
        data["discogs"] = fetch_discogs_data(&client, &rg);
        data["release_group"] = rg;
    } else {
        let rel_url = format!("https://musicbrainz.org/ws/2/release/{entity_id}?inc=labels+release-groups+url-rels+recordings+artist-credits+media&fmt=json");
        let release: Value = client.get(rel_url).send()?.json()?;
        
        let rg_id = release.get("release-group").and_then(|rg| rg.get("id")).and_then(|id| id.as_str()).unwrap_or("");
        let rg = if !rg_id.is_empty() {
            let rg_url = format!("https://musicbrainz.org/ws/2/release-group/{rg_id}?inc=tags+artist-credits+url-rels&fmt=json");
            client.get(rg_url).send()?.json().ok()
        } else {
            None
        };

        let mut discogs_info = fetch_discogs_data(&client, &release);
        
        if discogs_info.get("urls").and_then(|u| u.as_object()).is_none_or(serde_json::Map::is_empty)
            && let Some(rg_obj) = &rg
        {
            discogs_info = fetch_discogs_data(&client, rg_obj);
        }

        data["discogs"] = discogs_info;
        data["release"] = release;
        data["release_group"] = rg.unwrap_or_else(|| json!({}));
    }

    fs::write(&cache_file, serde_json::to_string_pretty(&data)?)?;
    Ok(data)
}

fn fetch_discogs_data(client: &reqwest::blocking::Client, mb_obj: &Value) -> Value {
    let mut discogs_url = String::new();

    if let Some(relations) = mb_obj.get("relations").and_then(|r| r.as_array()) {
        for rel in relations {
            if let Some(url_str) = rel.get("url").and_then(|u| u.get("resource")).and_then(|s| s.as_str())
                && (url_str.contains("discogs.com/release/") || url_str.contains("discogs.com/master/"))
            {
                discogs_url = url_str.to_string();
                break;
            }
        }
    }

    if discogs_url.is_empty() { return json!({}); }

    let rel_re = Regex::new(r"release/(\d+)").unwrap();
    let mas_re = Regex::new(r"master/(\d+)").unwrap();

    let token_opt = get_discogs_token();
    let auth_header = token_opt.map(|t| format!("Discogs token={t}"));

    let mut urls = json!({});
    let mut result = json!({});

    if let Some(caps) = rel_re.captures(&discogs_url) {
        let id = caps.get(1).unwrap().as_str();
        urls["release"] = json!(format!("https://discogs.com/release/{id}"));
        
        if let Some(auth) = &auth_header
            && let Ok(resp) = client.get(format!("https://api.discogs.com/releases/{id}")).header("Authorization", auth).send()
            && let Ok(rel_data) = resp.json::<Value>()
        {
            result["release"] = rel_data.clone();
            if let Some(m_id) = rel_data.get("master_id").and_then(serde_json::Value::as_i64) {
                urls["master"] = json!(format!("https://discogs.com/master/{m_id}"));
                if let Ok(m_resp) = client.get(format!("https://api.discogs.com/masters/{m_id}")).header("Authorization", auth).send()
                    && let Ok(m_data) = m_resp.json::<Value>()
                {
                    result["master"] = m_data;
                }
            }
        }
    } else if let Some(caps) = mas_re.captures(&discogs_url) {
        let id = caps.get(1).unwrap().as_str();
        urls["master"] = json!(format!("https://discogs.com/master/{id}"));

        if let Some(auth) = &auth_header
            && let Ok(resp) = client.get(format!("https://api.discogs.com/masters/{id}")).header("Authorization", auth).send()
            && let Ok(m_data) = resp.json::<Value>()
        {
            result["master"] = m_data;
        }
    }

    result["urls"] = urls;
    result
}
