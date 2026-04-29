use crate::adapters::hdc;
use crate::build::{BuildArgs, handle_build};
use crate::config::Config;
use crate::project::{Module, ModuleType};
use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use clap_complete::engine::ArgValueCompleter;

#[derive(Debug, Clone, ValueEnum)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn as_hilog_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "D,I,W,E,F",
            LogLevel::Info => "I,W,E,F",
            LogLevel::Warn => "W,E,F",
            LogLevel::Error => "E,F",
            LogLevel::Fatal => "F",
        }
    }
}
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Module name (format: module or module@target)
    #[arg(short, long, add = ArgValueCompleter::new(crate::completion::complete_runnable_modules))]
    pub module: Option<String>,

    /// Target device (name or ID). If not provided, heco will auto-select if only 1 device is online.
    #[arg(long, add = ArgValueCompleter::new(crate::completion::complete_devices))]
    pub device: Option<String>,

    /// Run in daemon mode (detach after launch)
    #[arg(short, long)]
    pub daemon: bool,

    /// Log level for hilog
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    pub app_log_level: LogLevel,
}

impl RunArgs {
    pub fn parse_module(&self) -> Option<(String, Option<String>)> {
        self.module.as_ref().map(|m| {
            if let Some(idx) = m.find('@') {
                let module_name = m[..idx].to_string();
                let target_name = m[idx + 1..].to_string();
                (module_name, Some(target_name))
            } else {
                (m.clone(), None)
            }
        })
    }
}

pub fn handle_run(args: RunArgs) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project = crate::project::load_project()?;
    let config = Config::load(Some(&project.root))?;

    // 1. Identify module
    let run_args = if args.module.is_none() {
        if let Some(module) = project.find_module_by_path(&current_dir) {
            println!("Auto-detected module: {}", module.name);
            RunArgs {
                module: Some(module.name.clone()),
                ..args
            }
        } else {
            let runnable_modules: Vec<&Module> = project
                .modules
                .iter()
                .filter(|m| matches!(m.module_type, ModuleType::Entry | ModuleType::Feature))
                .collect();

            if runnable_modules.len() == 1 {
                let module = runnable_modules[0];
                println!("Auto-selected module: {}", module.name);
                RunArgs {
                    module: Some(module.name.clone()),
                    ..args
                }
            } else {
                args
            }
        }
    } else {
        args
    };

    let (module_name, target_name) = match run_args.parse_module() {
        Some(m) => m,
        None => {
            bail!(
                "No module specified or detected. Please specify a module using -m or --module <name>"
            );
        }
    };
    let target_name = target_name.unwrap_or_else(|| "default".to_string());

    let main_module = match project.modules.iter().find(|m| m.name == module_name) {
        Some(m) => {
            if !matches!(m.module_type, ModuleType::Entry | ModuleType::Feature) {
                bail!(
                    "Module '{}' is of type '{:?}', which is not runnable. Please specify an entry or feature module.",
                    module_name,
                    m.module_type
                );
            }
            m
        }
        None => {
            let available_modules = project
                .modules
                .iter()
                .filter(|m| matches!(m.module_type, ModuleType::Entry | ModuleType::Feature))
                .map(|m| format!("  - {}", m.name))
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "Module '{}' not found in project.\nAvailable runnable modules:\n{}",
                module_name,
                available_modules
            )
        }
    };

    // 2. Select Device
    let target_device_id = select_device(&config, &run_args.device)?;
    let is_emulator =
        target_device_id.contains("127.0.0.1") || target_device_id.contains("localhost");

    // 3. Build (call heco build)
    println!("Building module {}...", module_name);
    let build_args = BuildArgs {
        modules: Some(vec![format!("{}@{}", module_name, target_name)]),
        debug: false,
        release: false,
        quiet: false,
        products: None,
    };
    handle_build(build_args); // Assume this succeeds and generates the artifacts

    // 4. Resolve artifacts and dependencies
    let bundle_name = project.get_bundle_name()?;
    let mut artifacts_to_install = Vec::new();
    let mut hsp_modules = Vec::new();

    // 4.1 Resolve HSP dependencies
    project.resolve_hsp_dependencies(main_module, &mut hsp_modules)?;

    // 4.2 Find paths for HSPs
    for hsp_mod in &hsp_modules {
        let path = find_artifact_path(&project.root, hsp_mod, &target_name, ".hsp", is_emulator)?;
        artifacts_to_install.push(path);
    }

    // 4.3 Find path for main HAP
    let main_hap_path = find_artifact_path(
        &project.root,
        main_module,
        &target_name,
        ".hap",
        is_emulator,
    )?;
    artifacts_to_install.push(main_hap_path);

    // 5. Install all
    println!("Installing artifacts to device {}...", target_device_id);
    let hdc_cmd = hdc::find_hdc_binary(&config)?;
    let mut install_cmd = Command::new(&hdc_cmd);
    install_cmd
        .arg("-t")
        .arg(&target_device_id)
        .arg("app")
        .arg("install");
    for path in &artifacts_to_install {
        install_cmd.arg(path);
    }

    let install_status = install_cmd.status()?;
    if !install_status.success() {
        bail!("Failed to install application");
    }

    // 6. Launch and log
    // We need the mainAbility name. Default to "EntryAbility" if not found.
    // Wait, let's just use aa start -a EntryAbility -b <bundleName>
    // A more robust way is to parse module.json5, but "EntryAbility" is standard.
    let main_ability = project
        .get_main_ability(main_module)
        .unwrap_or_else(|_| "EntryAbility".to_string());

    println!("Launching {}/{}...", bundle_name, main_ability);
    let launch_status = Command::new(&hdc_cmd)
        .arg("-t")
        .arg(&target_device_id)
        .arg("shell")
        .arg("aa")
        .arg("start")
        .arg("-a")
        .arg(&main_ability)
        .arg("-b")
        .arg(&bundle_name)
        .status()?;

    if !launch_status.success() {
        bail!("Failed to launch application");
    }

    // If daemon mode, exit now
    if run_args.daemon {
        println!("Application launched in daemon mode.");
        return Ok(());
    }

    // 7. Log streaming
    println!("Streaming logs for {}...", bundle_name);

    // Clear logs first
    let _ = Command::new(&hdc_cmd)
        .arg("-t")
        .arg(&target_device_id)
        .arg("shell")
        .arg("hilog")
        .arg("-r")
        .status();

    // Spawn hilog
    let log_level = run_args.app_log_level.as_hilog_str();

    // We can't easily filter by bundleName directly in hilog without grep in the shell,
    // but we can filter by domain or just use grep. Since we are running hdc shell, we can do:
    // hdc shell "hilog -L I | grep -E 'bundleName|FaultLogger'"
    // Alternatively, just stream and let the user see it, or filter in Rust.
    // Filtering in Rust is cleaner and cross-platform.
    let mut hilog_cmd = Command::new(&hdc_cmd)
        .arg("-t")
        .arg(&target_device_id)
        .arg("shell")
        .arg("hilog")
        .arg("-L")
        .arg(log_level)
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = hilog_cmd
        .stdout
        .take()
        .expect("Failed to capture hilog stdout");

    let hilog_child = std::sync::Arc::new(std::sync::Mutex::new(hilog_cmd));

    // Setup Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    let bundle_clone = bundle_name.clone();
    let target_clone = target_device_id.clone();
    let hdc_clone = hdc_cmd.clone();

    ctrlc::set_handler(move || {
        println!("\nStopping application {}...", bundle_clone);
        let _ = Command::new(&hdc_clone)
            .arg("-t")
            .arg(&target_clone)
            .arg("shell")
            .arg("aa")
            .arg("force-stop")
            .arg(&bundle_clone)
            .output();
        r.store(false, std::sync::atomic::Ordering::SeqCst);
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // Background thread to monitor app process
    let monitor_running = running.clone();
    let monitor_hdc = hdc_cmd.clone();
    let monitor_target = target_device_id.clone();
    let monitor_bundle = bundle_name.clone();
    let monitor_hilog = hilog_child.clone();

    std::thread::spawn(move || {
        // Give the app a moment to fully spawn before we start polling
        std::thread::sleep(std::time::Duration::from_secs(2));

        while monitor_running.load(std::sync::atomic::Ordering::SeqCst) {
            let output = Command::new(&monitor_hdc)
                .arg("-t")
                .arg(&monitor_target)
                .arg("shell")
                .arg(format!("pidof {}", monitor_bundle))
                .output();

            match output {
                Ok(out) => {
                    let pid_str = String::from_utf8_lossy(&out.stdout);
                    if pid_str.trim().is_empty() {
                        // App process is no longer running
                        // Wait a short moment to ensure any trailing crash logs (like FaultLogger) are flushed
                        std::thread::sleep(std::time::Duration::from_secs(1));

                        if monitor_running.load(std::sync::atomic::Ordering::SeqCst) {
                            println!(
                                "\nApplication process '{}' exited on device.",
                                monitor_bundle
                            );
                            monitor_running.store(false, std::sync::atomic::Ordering::SeqCst);
                            // Kill hilog to unblock the main log reading loop
                            if let Ok(mut child) = monitor_hilog.lock() {
                                let _ = child.kill();
                            }
                        }
                        break;
                    }
                }
                Err(_) => break,
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });

    use std::io::BufRead;
    let reader = std::io::BufReader::new(stdout);
    for line in reader.lines() {
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        if let Ok(line) = line {
            // Filter logic: show if contains bundleName or FaultLogger
            if line.contains(&bundle_name) || line.contains("FaultLogger") {
                println!("{}", line);
            }
        }
    }

    if let Ok(mut child) = hilog_child.lock() {
        let _ = child.kill();
    }
    Ok(())
}

fn select_device(config: &Config, device_arg: &Option<String>) -> Result<String> {
    let devices = hdc::list_targets(config)?;
    if devices.is_empty() {
        bail!("No active devices found. Please start an emulator or connect a physical device.");
    }

    if let Some(specified) = device_arg {
        for (name, id) in &devices {
            if id == specified || name.contains(specified) {
                return Ok(id.clone());
            }
        }
        bail!(
            "Device '{}' not found.\nAvailable devices:\n{}",
            specified,
            format_device_list(&devices)
        );
    }

    if devices.len() == 1 {
        let (name, id) = &devices[0];
        println!("Auto-selected device: {} ({})", name, id);
        return Ok(id.clone());
    }

    bail!(
        "Multiple devices found. Please specify a target device using `--device <Name>` or `--device <ID>`.\nAvailable devices:\n{}",
        format_device_list(&devices)
    );
}

fn format_device_list(devices: &[(String, String)]) -> String {
    devices
        .iter()
        .map(|(name, id)| format!("  - {} ({})", name, id))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_artifact_path(
    project_root: &Path,
    module: &Module,
    target: &str,
    ext: &str,
    is_emulator: bool,
) -> Result<PathBuf> {
    let outputs_dir = project_root
        .join(&module.src_path)
        .join("build")
        .join(target)
        .join("outputs")
        .join("default");

    if !outputs_dir.exists() {
        bail!(
            "Build outputs directory not found for module '{}': {}",
            module.name,
            outputs_dir.display()
        );
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(&outputs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|s| s.to_str()) == Some(ext.trim_start_matches('.'))
        {
            candidates.push(path);
        }
    }

    if candidates.is_empty() {
        bail!(
            "No {} artifacts found for module '{}' in {}",
            ext,
            module.name,
            outputs_dir.display()
        );
    }

    // Sort by modified time descending
    candidates.sort_by_key(|a| {
        std::cmp::Reverse(
            a.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });

    let signed = candidates
        .iter()
        .find(|p| p.to_string_lossy().contains("-signed"));
    let unsigned = candidates
        .iter()
        .find(|p| p.to_string_lossy().contains("-unsigned"));

    if !is_emulator {
        // Real device requires signed
        if let Some(p) = signed {
            Ok(p.clone())
        } else {
            bail!(
                "Target device is a real device, but no signed artifact found for '{}'. Real devices cannot install unsigned packages.",
                module.name
            );
        }
    } else {
        // Emulator prefers signed, fallback to unsigned, fallback to first
        if let Some(p) = signed {
            return Ok(p.clone());
        }
        if let Some(p) = unsigned {
            return Ok(p.clone());
        }
        Ok(candidates[0].clone())
    }
}
