use crate::config::{Config, ConfigFile, DevEcoConfig};
use crate::output;
use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct SdkPkg {
    data: SdkPkgData,
}

#[derive(Debug, Deserialize)]
struct SdkPkgData {
    #[serde(rename = "apiVersion")]
    api_version: String,
    version: String,
}

#[derive(Args, Debug)]
pub struct EnvArgs {
    #[command(subcommand)]
    pub command: EnvCommands,
}

#[derive(Subcommand, Debug)]
pub enum EnvCommands {
    /// Add a DevEco Studio path
    Add {
        /// Path to DevEco Studio
        #[arg(value_hint = clap::ValueHint::DirPath)]
        path: String,
        /// Set as the default path
        #[arg(long)]
        default: bool,
    },
    /// Remove a DevEco Studio path or version
    Remove {
        /// The path, API version (e.g. 12), or exact version (e.g. 5.0.0.123) to remove
        target: String,
    },
    /// List current environment configurations
    List,
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(path))
    } else if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(mut home) = dirs::home_dir() {
            home.push(stripped);
            home
        } else {
            PathBuf::from(path)
        }
    } else {
        PathBuf::from(path)
    }
}

fn get_config_file_path() -> PathBuf {
    crate::config::Config::global_path().expect("Failed to get global config path")
}

fn load_config_file() -> Result<ConfigFile> {
    let path = get_config_file_path();
    if !path.exists() {
        return Ok(ConfigFile::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config_file: ConfigFile = toml::from_str(&content)?;
    Ok(config_file)
}

fn save_config_file(config_file: &ConfigFile) -> Result<()> {
    let path = get_config_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string(config_file)?;
    std::fs::write(&path, content)?;
    Ok(())
}

fn get_sdk_version(root_path: &std::path::Path) -> Result<(String, String)> {
    #[cfg(target_os = "macos")]
    let sdk_pkg_path = root_path
        .join("Contents")
        .join("sdk")
        .join("default")
        .join("sdk-pkg.json");

    #[cfg(target_os = "windows")]
    let sdk_pkg_path = root_path.join("sdk").join("default").join("sdk-pkg.json");

    if !sdk_pkg_path.exists() {
        bail!("Could not find sdk-pkg.json at {:?}", sdk_pkg_path);
    }

    let content = std::fs::read_to_string(&sdk_pkg_path).context("Failed to read sdk-pkg.json")?;
    let sdk_pkg: SdkPkg = serde_json::from_str(&content).context("Failed to parse sdk-pkg.json")?;

    Ok((sdk_pkg.data.api_version, sdk_pkg.data.version))
}

fn handle_add(path: &str, default: bool) -> Result<()> {
    let mut config_file = load_config_file()?;
    let mut env_config = config_file.env.unwrap_or_default();

    let root_path = expand_path(path);

    if !root_path.exists() {
        bail!(
            "Path does not exist: {} (expanded to {})",
            path,
            root_path.display()
        );
    }

    let (api_version, version) = get_sdk_version(&root_path)?;

    if !output::is_quiet() {
        output::status("Detected", format!("API {api_version}, version {version}"));
    }

    env_config.deveco_studios.insert(
        api_version.clone(),
        DevEcoConfig {
            path: root_path.clone(),
            version: version.clone(),
        },
    );

    if default {
        env_config.default_deveco_studio = Some(root_path.clone());
        if !output::is_quiet() {
            output::status("Setting", "default DevEco Studio");
        }
    } else if env_config.default_deveco_studio.is_none() {
        // If it's the first one, optionally make it default automatically?
        // Let's keep it explicit as per user requirement, but it's good practice.
        // We'll stick to explicit: only if --default is passed.
    }

    config_file.env = Some(env_config);
    save_config_file(&config_file)?;

    output::stdout_line(format!("Added DevEco Studio path: {}", path));
    Ok(())
}

fn handle_remove(target: &str) -> Result<()> {
    let mut config_file = load_config_file()?;
    let mut env_config = config_file.env.unwrap_or_default();

    let mut keys_to_remove = Vec::new();
    let expanded_target = expand_path(target);

    for (api_version, config) in &env_config.deveco_studios {
        if api_version == target
            || config.version == target
            || config.path.to_string_lossy() == target
            || config.path == expanded_target
        {
            keys_to_remove.push(api_version.clone());
        }
    }

    if keys_to_remove.is_empty() {
        output::warning(format!(
            "no matching configuration found for target: {}",
            target
        ));
        return Ok(());
    }

    for key in keys_to_remove {
        if let Some(removed) = env_config.deveco_studios.remove(&key) {
            output::stdout_line(format!(
                "Removed configuration: API version {}, path {:?}",
                key, removed.path
            ));

            if let Some(default_path) = &env_config.default_deveco_studio
                && default_path == &removed.path
            {
                env_config.default_deveco_studio = None;
                if !output::is_quiet() {
                    output::status("Removed", "from default DevEco Studio path");
                }
            }
        }
    }

    config_file.env = Some(env_config);
    save_config_file(&config_file)?;

    Ok(())
}

use owo_colors::OwoColorize;

fn get_deveco_studio_version(path: &std::path::Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    let product_info_path = path
        .join("Contents")
        .join("Resources")
        .join("product-info.json");

    #[cfg(target_os = "windows")]
    let product_info_path = path.join("product-info.json");

    if let Ok(content) = std::fs::read_to_string(&product_info_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(version) = json.get("version").and_then(|v| v.as_str())
    {
        return Some(version.to_string());
    }
    None
}

fn handle_list() -> Result<()> {
    let project_root = crate::project::find_project_root();
    let config = Config::load(project_root.as_ref()).unwrap_or_default();
    let resolved_path = config.resolved_deveco_studio;

    let config_file = load_config_file()?;
    let env_config = config_file.env.unwrap_or_default();

    if env_config.deveco_studios.is_empty() {
        if let Some(resolved) = &resolved_path {
            let app_version = get_deveco_studio_version(resolved)
                .map(|v| format!(" v{}", v.cyan()))
                .unwrap_or_default();

            let sdk_version = get_sdk_version(resolved)
                .map(|(api, v)| format!("{}({})", v, api))
                .unwrap_or_else(|_| "unknown".to_string());

            output::stdout_line(format!(
                "* {}{} [{}] {}",
                sdk_version.cyan(),
                app_version,
                "auto".yellow(),
                resolved.display().to_string().dimmed()
            ));
        } else {
            output::stdout_line(format!(
                "  {}",
                "No DevEco Studio versions configured or detected.".dimmed()
            ));
        }
        return Ok(());
    }

    for (api_version, dev_config) in &env_config.deveco_studios {
        let is_resolved = if let Some(ref r) = resolved_path {
            r == &dev_config.path
        } else {
            false
        };

        let is_default = if let Some(ref d) = env_config.default_deveco_studio {
            d == &dev_config.path
        } else {
            false
        };

        let marker = if is_resolved {
            "*".green().bold().to_string()
        } else {
            " ".to_string()
        };

        let sdk_version_str = format!("{}({})", dev_config.version, api_version)
            .cyan()
            .to_string();

        let app_version = get_deveco_studio_version(&dev_config.path)
            .map(|v| format!(" v{}", v.cyan()))
            .unwrap_or_default();

        let mut tags = Vec::new();
        if is_default {
            tags.push("default");
        }

        let tag_str = if !tags.is_empty() {
            format!(" [{}]", tags.join(", ").yellow())
        } else {
            "".to_string()
        };

        output::stdout_line(format!(
            "{} {}{}{} {}",
            marker,
            sdk_version_str,
            app_version,
            tag_str,
            dev_config.path.display().to_string().dimmed()
        ));
    }

    let is_resolved_in_list = if let Some(ref r) = resolved_path {
        env_config.deveco_studios.values().any(|v| &v.path == r)
    } else {
        false
    };
    if !is_resolved_in_list {
        if let Some(resolved) = &resolved_path {
            let app_version = get_deveco_studio_version(resolved)
                .map(|v| format!(" v{}", v.cyan()))
                .unwrap_or_default();

            let sdk_version = get_sdk_version(resolved)
                .map(|(api, v)| format!("{}({})", v, api))
                .unwrap_or_else(|_| "unknown".to_string());

            output::stdout_line(format!(
                "* {}{} [{}] {}",
                sdk_version.cyan(),
                app_version,
                "auto".yellow(),
                resolved.display().to_string().dimmed()
            ));
        }
    } else {
        // If the resolved path is in the list, we might still want to show the auto-detected path
        // if it's different from all configured paths, just to inform the user it exists.
        if let Some(auto_detected) = crate::config::Config::get_auto_detected_deveco_studio() {
            let is_auto_in_list = env_config.deveco_studios.values().any(|v| {
                // normalize both paths to avoid false negatives due to trailing slashes
                let v_str = v.path.to_string_lossy().trim_end_matches('/').to_string();
                let auto_str = auto_detected
                    .to_string_lossy()
                    .trim_end_matches('/')
                    .to_string();
                v_str == auto_str
            });
            if !is_auto_in_list {
                let app_version = get_deveco_studio_version(&auto_detected)
                    .map(|v| format!(" v{}", v.cyan()))
                    .unwrap_or_default();

                let sdk_version = get_sdk_version(&auto_detected)
                    .map(|(api, v)| format!("{}({})", v, api))
                    .unwrap_or_else(|_| "unknown".to_string());

                output::stdout_line(format!(
                    "  {}{} [{}] {}",
                    sdk_version.cyan(),
                    app_version,
                    "auto".yellow(),
                    auto_detected.display().to_string().dimmed()
                ));
            }
        }
    }

    Ok(())
}

pub fn handle_env(args: EnvArgs) {
    let result = match args.command {
        EnvCommands::Add { path, default } => handle_add(&path, default),
        EnvCommands::Remove { target } => handle_remove(&target),
        EnvCommands::List => handle_list(),
    };

    if let Err(e) = result {
        output::error(e);
    }
}
