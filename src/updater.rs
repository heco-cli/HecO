use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_FILE_NAME: &str = "update_cache.json";
const CACHE_VALID_SECONDS: u64 = 24 * 60 * 60; // 24 hours
const CRATES_IO_API: &str = "https://crates.io/api/v1/crates/heco";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateCache {
    pub latest_version: String,
    pub last_check: u64,
}

fn get_cache_path() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".config");
    path.push("heco");
    fs::create_dir_all(&path).ok()?;
    path.push(CACHE_FILE_NAME);
    Some(path)
}

pub fn get_cached_update() -> Option<String> {
    let path = get_cache_path()?;
    let content = fs::read_to_string(&path).ok()?;
    let cache: UpdateCache = serde_json::from_str(&content).ok()?;

    // Even if cache is expired, we can still return the version it knows
    // The main process will spawn a background check if it's expired.
    let current_version = env!("CARGO_PKG_VERSION");

    if is_newer_version(current_version, &cache.latest_version) {
        Some(cache.latest_version)
    } else {
        None
    }
}

pub fn should_check_update() -> bool {
    if let Some(path) = get_cache_path()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(cache) = serde_json::from_str::<UpdateCache>(&content)
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now >= cache.last_check && now - cache.last_check < CACHE_VALID_SECONDS {
            return false;
        }
    }
    true
}

#[derive(Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    crate_data: CrateData,
}

#[derive(Deserialize)]
struct CrateData {
    max_version: String,
}

pub fn fetch_latest_version() -> anyhow::Result<String> {
    let agent = ureq::builder()
        .user_agent(concat!("heco/", env!("CARGO_PKG_VERSION")))
        .build();

    let resp = agent.get(CRATES_IO_API).call();
    match resp {
        Ok(response) => {
            let crate_resp: CrateResponse = response.into_json()?;
            Ok(crate_resp.crate_data.max_version)
        }
        Err(e) => {
            // If the crate doesn't exist yet (404), return an error
            anyhow::bail!("{e}");
        }
    }
}

pub fn update_cache(latest_version: &str) -> anyhow::Result<()> {
    if let Some(path) = get_cache_path() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let cache = UpdateCache {
            latest_version: latest_version.to_string(),
            last_check: now,
        };
        fs::write(&path, serde_json::to_string(&cache)?)?;
    }
    Ok(())
}

pub fn check_and_cache_update() -> anyhow::Result<()> {
    if !should_check_update() {
        return Ok(());
    }

    let latest_version = fetch_latest_version()?;
    update_cache(&latest_version)?;

    Ok(())
}

fn is_newer_version(current: &str, latest: &str) -> bool {
    let current_parts: Vec<&str> = current.split('.').collect();
    let latest_parts: Vec<&str> = latest.split('.').collect();

    for i in 0..std::cmp::min(current_parts.len(), latest_parts.len()) {
        let curr_num: u32 = current_parts[i].parse().unwrap_or(0);
        let latest_num: u32 = latest_parts[i].parse().unwrap_or(0);

        if latest_num > curr_num {
            return true;
        } else if latest_num < curr_num {
            return false;
        }
    }

    latest_parts.len() > current_parts.len()
}
