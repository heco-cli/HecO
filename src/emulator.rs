use crate::command::CommandRunner;
use crate::config::Config;
use crate::output;
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Args, Debug)]
pub struct EmulatorArgs {
    #[command(subcommand)]
    pub command: EmulatorCommands,
}

#[derive(Subcommand, Debug)]
pub enum EmulatorCommands {
    /// Start an emulator instance
    Start(StartArgs),
    /// Stop a running emulator
    Stop(StopArgs),
    /// List all available emulators
    List(ListArgs),
}

#[derive(Args, Debug)]
pub struct StartArgs {
    /// Name of the emulator to start
    #[arg(add = ArgValueCompleter::new(crate::completion::complete_emulators))]
    pub name: String,
}

#[derive(Args, Debug)]
pub struct StopArgs {
    /// Name of the emulator to stop
    #[arg(add = ArgValueCompleter::new(crate::completion::complete_emulators))]
    pub name: String,
    /// Force stop (kill process)
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct ListArgs {}

pub fn handle_emulator(args: EmulatorArgs) -> Result<()> {
    match args.command {
        EmulatorCommands::Start(start_args) => handle_start(start_args),
        EmulatorCommands::Stop(stop_args) => handle_stop(stop_args),
        EmulatorCommands::List(list_args) => handle_list(list_args),
    }
}

fn handle_start(args: StartArgs) -> Result<()> {
    output::status("Starting", format!("({})", args.name));

    let emulator_cmd = find_emulator_binary()?;
    let config = Config::load(None)?;

    // 构建命令参数
    let mut cmd_args: Vec<String> = vec!["-hvd".to_string(), args.name.clone()];

    // 添加模拟器实例路径
    if let Some(instance_path) = config.get_emulator_instance_path() {
        cmd_args.extend_from_slice(&[
            "-path".to_string(),
            instance_path.to_str().unwrap().to_string(),
        ]);
    } else {
        bail!("Could not find emulator instance path. Please configure it via 'heco env'");
    }

    // 添加模拟器镜像路径
    if let Some(image_root) = config.get_emulator_image_root() {
        cmd_args.extend_from_slice(&[
            "-imageRoot".to_string(),
            image_root.to_str().unwrap().to_string(),
        ]);
    } else {
        bail!("Could not find emulator image root. Please configure it via 'heco env'");
    }

    // 创建 CommandRunner
    let runner = CommandRunner::new(std::env::current_dir()?);

    // 执行命令并设置超时
    let timeout = Duration::from_secs(2);
    let start_time = std::time::Instant::now();

    // 启动命令执行
    let cmd_args_slice: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    let output = runner.run_captured_merged_with_timeout(
        emulator_cmd.to_str().unwrap(),
        &cmd_args_slice,
        Some(timeout),
    )?;

    let elapsed = start_time.elapsed();
    let elapsed_seconds = elapsed.as_secs() as f64 + elapsed.subsec_millis() as f64 / 1000.0;

    // 处理执行结果
    let output_str = String::from_utf8_lossy(&output.stdout);

    let already_running =
        output_str.contains("already exist") || output_str.contains("already running");

    if already_running {
        output::warning(format!("emulator '{}' is already running", args.name));
    } else if output.status.success() {
        output::status("Finished", format!("in {:.2}s", elapsed_seconds));
    } else {
        bail!("Failed to start emulator: {}", output_str.trim());
    }

    Ok(())
}

fn handle_stop(args: StopArgs) -> Result<()> {
    output::status("Stopping", format!("({})", args.name));

    let emulator_cmd = find_emulator_binary()?;
    let start_time = std::time::Instant::now();

    let mut cmd = Command::new(&emulator_cmd);
    cmd.arg("-stop").arg(&args.name);

    if args.force {
        output::status("Stopping", "emulator");
    }

    let output = cmd.output()?;
    let elapsed = start_time.elapsed();
    let elapsed_seconds = elapsed.as_secs() as f64 + elapsed.subsec_millis() as f64 / 1000.0;

    if output.status.success() {
        output::status("Finished", format!("in {:.2}s", elapsed_seconds));
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If emulator is already stopped, it's not an error
        if stderr.contains("not running") || stderr.contains("stopped") {
            output::warning(format!("emulator '{}' is already stopped", args.name));
        } else {
            bail!("Failed to stop emulator: {}", stderr);
        }
    }

    Ok(())
}

pub fn get_emulator_list() -> Result<Vec<String>> {
    let emulator_cmd = find_emulator_binary()?;

    let mut cmd = Command::new(&emulator_cmd);
    cmd.arg("-list");

    let output = cmd.output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut emulators = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if !line.is_empty() {
                emulators.push(line.to_string());
            }
        }

        Ok(emulators)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to list emulators: {}", stderr);
    }
}

fn handle_list(_args: ListArgs) -> Result<()> {
    let emulators = get_emulator_list()?;

    if emulators.is_empty() {
        output::stdout_line("  No emulators found.");
    } else {
        for name in emulators {
            output::stdout_line(format!("  {}", name));
        }
    }

    Ok(())
}

/// 查找 Emulator 可执行文件路径
fn find_emulator_binary() -> Result<PathBuf> {
    // 从 heco 配置中读取
    let config = Config::load(None)?;

    if let Some(emulator_path) = config.emulator_path() {
        return Ok(emulator_path);
    }

    bail!(
        "Could not find Emulator binary. Please ensure DevEco Studio is installed and configured in heco env."
    )
}
