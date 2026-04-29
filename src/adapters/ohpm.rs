use crate::command::CommandRunner;
use crate::config::Config;
use crate::progress::StatusBar;
use anstream::println;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LogType {
    Warning,
    Error,
}

// 预定义日志前缀映射
static LOG_PREFIX_MAP: LazyLock<HashMap<LogType, Vec<&'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(LogType::Warning, vec!["WARN:", "warning:", "ohpm WARN:"]);
    m.insert(LogType::Error, vec!["ERROR:", "ERR!", "error:"]);
    m
});

/// 识别行是否匹配指定的日志类型，如果匹配则返回 (LogType, 剥离前缀后的内容)
fn parse_log_type(line: &str) -> Option<(LogType, String)> {
    let line_trim = line.trim();
    for (log_type, prefixes) in LOG_PREFIX_MAP.iter() {
        for prefix in prefixes {
            if line_trim.starts_with(prefix) {
                // 如果找到匹配，截取掉前缀并返回
                let content = line_trim.strip_prefix(prefix).unwrap().trim().to_string();
                return Some((*log_type, content));
            }
        }
    }
    None
}

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

    if quiet {
        let output = runner.run_captured_merged(ohpm_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: ohpm install failed".red());
        }
    } else {
        let mut last_log_type: Option<LogType> = None;

        runner.run_with_handler(ohpm_path_str, &program_args, |line| {
            let processed = anstream::adapter::strip_str(line).to_string();
            if processed.trim().is_empty() {
                return;
            }

            let mut content_line = processed.trim();
            // 去除所有连续的 "ohpm " 前缀，解决 "ohpm  ohpm WARN:" 这种重复情况
            while content_line.starts_with("ohpm ") {
                content_line = content_line.strip_prefix("ohpm ").unwrap().trim_start();
            }

            // 尝试解析为警告/错误
            if let Some((log_type, content)) = parse_log_type(content_line) {
                last_log_type = Some(log_type);
                let output = match log_type {
                    LogType::Warning => format!("{}: {}", "warning".yellow().bold(), content),
                    LogType::Error => format!("{}: {}", "error".red().bold(), content),
                };
                if let Some(b) = bar {
                    b.println(&output);
                } else {
                    println!("{}", output);
                }
            } else {
                // 检查是否是延续行（以空白开头且上一行是警告/错误）
                if line.starts_with(char::is_whitespace) && last_log_type.is_some() {
                    // 延续，直接打印原始内容
                    if let Some(b) = bar {
                        b.println(&processed);
                    } else {
                        println!("{}", processed);
                    }
                } else {
                    // 普通行，重置延续状态
                    last_log_type = None;
                    let output = format!("{:>12} {}", "ohpm".green().bold(), content_line);
                    if let Some(b) = bar {
                        b.println(&output);
                    } else {
                        println!("{}", output);
                    }
                }
            }
        })?;
    }

    Ok(())
}
