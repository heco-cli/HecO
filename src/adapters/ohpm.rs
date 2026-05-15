use crate::adapters::output_parser::{LogType, parse_log_type, strip_repeated_prefix};
use crate::command::{CommandOutputEvent, CommandRunner};
use crate::config::Config;
use crate::output;
use crate::progress::StatusBar;
use owo_colors::OwoColorize;
use std::path::Path;

const OHPM_LOG_PREFIXES: &[(LogType, &[&str])] = &[
    (LogType::Warning, &["WARN:", "warning:", "ohpm WARN:"]),
    (LogType::Error, &["ERROR:", "ERR!", "error:"]),
];

pub fn install(
    project_root: &Path,
    config: &Config,
    quiet: bool,
    bar: Option<&StatusBar>,
) -> anyhow::Result<()> {
    let ohpm_path = config
        .ohpm_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 ohpm 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let node_bin = node_path.parent().unwrap();
    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", node_bin.to_str().unwrap_or(""), current_path);

    let runner = CommandRunner::new(project_root.to_path_buf())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""))
        .env("PATH", &new_path);

    let program_args = vec!["install", "--all"];

    let ohpm_path_str = ohpm_path.to_str().unwrap_or("ohpm");

    let mut last_log_type: Option<LogType> = None;

    runner.run_with_events(ohpm_path_str, &program_args, |event| {
        let line = match event {
            CommandOutputEvent::Line(line) | CommandOutputEvent::Overwrite(line) => line,
        };

        let processed = anstream::adapter::strip_str(&line).to_string();
        if processed.trim().is_empty() {
            return;
        }

        let content_line = strip_repeated_prefix(processed.trim(), "ohpm ");
        if should_skip_success_line(content_line) {
            return;
        }

        if let Some((log_type, content)) = parse_log_type(content_line, OHPM_LOG_PREFIXES) {
            last_log_type = Some(log_type);
            let output = match log_type {
                LogType::Warning => format!("{}: {}", "warning".yellow().bold(), content),
                LogType::Error => format!("{}: {}", "error".red().bold(), content),
            };
            emit_line(if quiet { None } else { bar }, &output);
        } else if line.starts_with(char::is_whitespace) && last_log_type.is_some() {
            emit_line(if quiet { None } else { bar }, &processed);
        } else {
            last_log_type = None;
            if !quiet {
                emit_line(bar, content_line);
            }
        }
    })?;

    Ok(())
}

fn emit_line(bar: Option<&StatusBar>, content: &str) {
    if let Some(bar) = bar {
        bar.println(content);
    } else {
        output::line(content);
    }
}

fn should_skip_success_line(content: &str) -> bool {
    content.starts_with("install completed in ")
}
