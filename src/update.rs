use clap::Parser;
use std::env;
use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
pub struct UpdateArgs {}

pub fn handle_update(_args: UpdateArgs) -> anyhow::Result<()> {
    println!("Checking for updates...");

    let current_version = env!("CARGO_PKG_VERSION");
    let latest_version = match crate::updater::fetch_latest_version() {
        Ok(v) => v,
        Err(e) => {
            anyhow::bail!("Failed to fetch latest version: {}", e);
        }
    };

    if !is_newer_version(current_version, &latest_version) {
        println!(
            "You are already using the latest version ({})",
            current_version
        );
        return Ok(());
    }

    println!(
        "Found new version: {} -> {}",
        current_version, latest_version
    );
    println!("Updating heco...");

    // Auto detect installation method based on executable path
    let exe_path = match env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            anyhow::bail!("Failed to get current executable path: {}", e);
        }
    };

    let exe_str = exe_path.to_string_lossy().to_lowercase();

    let mut command = if exe_str.contains(".cargo/bin") || exe_str.contains(".cargo\\bin") {
        println!("Detected installation method: Cargo");
        let mut cmd = Command::new("cargo");
        cmd.args(["install", "heco"]);
        cmd
    } else if exe_str.contains("homebrew")
        || exe_str.contains("linuxbrew")
        || exe_str.contains("cellar")
    {
        println!("Detected installation method: Homebrew");
        let mut cmd = Command::new("brew");
        cmd.args(["upgrade", "heco"]);
        cmd
    } else if exe_str.contains("windowsapps")
        || exe_str.contains("winget")
        || exe_str.contains("winget\\packages")
    {
        println!("Detected installation method: Winget");
        let mut cmd = Command::new("winget");
        cmd.args(["upgrade", "HecO-CLI.HecO", "-s", "winget"]);
        cmd
    } else {
        println!(
            "⚠️  Warning: Could not automatically detect the installation method. Please update heco manually or download from https://github.com/heco-cli/heco/releases/latest"
        );
        return Ok(());
    };

    let status = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        println!("✨ Successfully updated heco to version {}", latest_version);

        // Update the cache so it doesn't prompt again immediately
        let _ = crate::updater::update_cache(&latest_version);
    } else {
        anyhow::bail!("Update failed with status: {}", status);
    }

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
