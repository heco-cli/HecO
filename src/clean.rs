use crate::adapters::hvigor;
use crate::config::Config;
use crate::output;
use crate::project::find_project_root;
use clap::Parser;
use clap_complete::engine::ArgValueCompleter;
use std::time::Instant;

#[derive(Parser, Debug, Clone)]
pub struct CleanArgs {
    /// Specific module to clean (e.g., entry)
    #[arg(long, short, add = ArgValueCompleter::new(crate::completion::complete_modules))]
    pub module: Option<String>,

    #[arg(skip)]
    pub quiet: bool,

    /// Uninstall the application from specific device(s) after cleaning local artifacts (comma separated)
    #[arg(long, value_delimiter = ',', conflicts_with = "with_all_devices", add = ArgValueCompleter::new(crate::completion::complete_devices))]
    pub with_devices: Option<Vec<String>>,

    /// Uninstall from all connected devices after cleaning local artifacts
    #[arg(long, conflicts_with = "with_devices")]
    pub with_all_devices: bool,
}

pub(crate) fn handle_clean(args: CleanArgs) -> anyhow::Result<()> {
    let project_root = find_project_root()
        .ok_or_else(|| anyhow::anyhow!("no project root found (build-profile.json5)"))?;

    let config = Config::load(Some(&project_root))
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let project = crate::project::load_project()
        .map_err(|e| anyhow::anyhow!("failed to load project: {e}"))?;

    let args = if args.module.is_none() {
        if let Ok(current_dir) = std::env::current_dir() {
            if let Some(module) = project.find_module_by_path(&current_dir) {
                CleanArgs {
                    module: Some(module.name.clone()),
                    quiet: args.quiet,
                    with_devices: args.with_devices.clone(),
                    with_all_devices: args.with_all_devices,
                }
            } else {
                args
            }
        } else {
            args
        }
    } else {
        args
    };

    let start = Instant::now();

    if !args.quiet {
        let target_display = match &args.module {
            Some(m) => format!("module {}", m),
            None => "project".to_string(),
        };
        output::status(
            "Cleaning",
            format!("{target_display} ({})", project_root.display()),
        );
    }

    match hvigor::clean(&args, &project_root, &config, None) {
        Ok(_) => {
            if !args.quiet {
                output::finished("", start.elapsed());
            }
        }
        Err(e) => {
            anyhow::bail!("clean failed: {e}");
        }
    }

    if (args.with_devices.is_some() || args.with_all_devices)
        && let Err(e) = handle_uninstall(&args, &project, &config)
    {
        anyhow::bail!("uninstall failed: {e}");
    }

    Ok(())
}

fn handle_uninstall(
    args: &CleanArgs,
    project: &crate::project::Project,
    config: &Config,
) -> anyhow::Result<()> {
    let bundle_name = project.get_bundle_name()?;

    let devices = crate::adapters::hdc::list_targets(config)?;
    if devices.is_empty() {
        anyhow::bail!("No active devices found to uninstall from.");
    }

    let mut target_devices: Vec<(String, String)> = Vec::new();

    if args.with_all_devices {
        target_devices = devices.clone();
    } else if let Some(specified_devices) = &args.with_devices {
        for specified in specified_devices {
            let specified = specified.trim();
            let mut found = false;
            for (name, id) in &devices {
                if id == specified || name.contains(specified) {
                    target_devices.push((name.clone(), id.clone()));
                    found = true;
                    break;
                }
            }
            if !found {
                anyhow::bail!(
                    "Device '{}' not found.\nAvailable devices:\n{}",
                    specified,
                    format_device_list(&devices)
                );
            }
        }
    }

    let hdc_cmd = crate::adapters::hdc::find_hdc_binary(config)?;

    for (device_name, device_id) in target_devices {
        if !args.quiet {
            output::status(
                "Uninstall",
                format!("{bundle_name} from {device_name} ({device_id})"),
            );
        }

        let status = std::process::Command::new(&hdc_cmd)
            .arg("-t")
            .arg(&device_id)
            .arg("app")
            .arg("uninstall")
            .arg(&bundle_name)
            .status()?;

        if !status.success() {
            anyhow::bail!("Failed to uninstall from {} ({})", device_name, device_id);
        }
    }

    Ok(())
}

fn format_device_list(devices: &[(String, String)]) -> String {
    devices
        .iter()
        .map(|(name, id)| format!("  - {} ({})", name, id))
        .collect::<Vec<_>>()
        .join("\n")
}
