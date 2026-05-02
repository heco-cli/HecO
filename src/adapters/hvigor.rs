use crate::build::BuildArgs;
use crate::clean::CleanArgs;
use crate::command::CommandRunner;
use crate::config::Config;
use crate::progress::StatusBar;
use crate::project::{ModuleType, load_project};
use anstream::println;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LogType {
    Warning,
    Error,
}

// 预定义日志前缀映射
static LOG_PREFIX_MAP: LazyLock<HashMap<LogType, Vec<&'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(
        LogType::Warning,
        // 注意顺序：将最长的前缀放在最前面，防止短前缀（如 "warning:"）提前被匹配
        vec![
            "WARN: WARN: ArkTS:WARN File:",
            "WARN: ArkTS:WARN File:",
            "WARN: ArkTS:WARN",
            "ArkTS:WARN File:",
            "WARN:",
            "ArkTS:WARN",
        ],
    );
    m.insert(LogType::Error, vec!["ERROR: ArkTS:ERROR", "ERROR:"]);
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

/// 运行命令并处理日志块
fn run_command_with_log_handling(
    runner: &CommandRunner,
    node_path_str: &str,
    program_args: &[&str],
    width: usize,
    bar: Option<&StatusBar>,
) -> anyhow::Result<()> {
    // 追踪连续行的状态
    let mut last_log_type: Option<LogType> = None;
    let mut first_line = true;

    runner.run_with_handler(node_path_str, program_args, |line| {
        // 立即处理这一行
        let mut processed_line = anstream::adapter::strip_str(line).to_string();

        if processed_line.trim().is_empty() {
            return;
        }

        // 如果是新块开头，重置为非延续状态，并去掉 "> hvigor " 前缀
        let is_block_header = processed_line.trim().starts_with("> hvigor ");

        if is_block_header {
            processed_line = processed_line
                .trim_start_matches("> hvigor ")
                .trim_start()
                .to_string();
            last_log_type = None;
        }

        // 尝试解析为警告/错误
        if let Some((log_type, content)) = parse_log_type(&processed_line) {
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
            if line.starts_with(char::is_whitespace)
                && let Some(_log_type) = last_log_type
            {
                // 延续，直接打印原始内容
                if let Some(b) = bar {
                    b.println(&processed_line);
                } else {
                    println!("{}", processed_line);
                }
            } else {
                // 普通行，重置延续状态
                last_log_type = None;

                let output = if is_block_header || first_line {
                    // 块的第一行，添加 hvigor 前缀
                    first_line = false;
                    format!(
                        "{:>width$} {}",
                        "hvigor".green().bold(),
                        processed_line,
                        width = width
                    )
                } else {
                    // 普通延续行
                    processed_line.clone()
                };

                if let Some(b) = bar {
                    b.println(&output);
                } else {
                    println!("{}", output);
                }
            }
        }
    })?;

    Ok(())
}

impl BuildArgs {
    pub fn to_command_args(&self, project_root: &PathBuf) -> anyhow::Result<Vec<String>> {
        let mut args = Vec::new();
        let project = load_project()?;
        if project.root != *project_root {
            anyhow::bail!("project root mismatch");
        }

        // Handle product parameter
        let product = if let Some(products) = &self.products {
            if !products.is_empty() {
                if self.modules.is_some() && products.len() > 1 {
                    anyhow::bail!("only one product is allowed when using --modules parameter");
                }
                let p = &products[0];
                project.validate_product(p)?;
                Some(p)
            } else {
                None
            }
        } else {
            None
        };

        if self.products.is_some() && self.modules.is_none() {
            // Only products specified, use assembleApp
            args.push("assembleApp".to_string());

            // Since build.rs now handles loop logic, self.products should only contain exactly 1 product
            if let Some(p) = product {
                args.push("-p".to_string());
                args.push(format!("product={}", p));
            }
        }

        // Handle modules parameter (whether products are specified or not)
        if self.modules.is_some() {
            let parsed_modules = self.parse_modules().unwrap_or_default();

            if parsed_modules.is_empty() {
                let mut tasks = resolve_tasks("", &None, project_root)?;
                args.append(&mut tasks);
            } else {
                let mut all_tasks = Vec::new();
                let mut module_names = Vec::new();

                for (module_name, target_name) in parsed_modules {
                    let mut tasks = resolve_tasks(&module_name, &target_name, project_root)?;
                    all_tasks.append(&mut tasks);
                    module_names.push(module_name);
                }

                all_tasks.sort();
                all_tasks.dedup();
                args.append(&mut all_tasks);

                args.push("-p".to_string());
                args.push(format!("module={}", module_names.join(",")));
            }

            // Add product parameter if specified
            if let Some(p) = product {
                args.push("-p".to_string());
                args.push(format!("product={}", p));
            }
        }

        let mode = if self.release { "release" } else { "debug" };
        args.push("-p".to_string());
        args.push(format!("buildMode={}", mode));

        // 默认禁用 daemon 以避免 uv_cwd 报错问题
        args.push("--no-daemon".to_string());

        Ok(args)
    }
}

pub fn sync(
    project_root: &Path,
    config: &Config,
    quiet: bool,
    width: usize,
    bar: Option<&StatusBar>,
) -> anyhow::Result<()> {
    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let hvigorw_js_path = config
        .hvigorw_js_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 hvigorw.js 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let java_path = config.java_path().ok_or_else(|| {
        anyhow::anyhow!("未找到 Java 路径，请确保 JAVA_HOME 环境变量已设置或 Java 在 PATH 中")
    })?;

    let java_home = java_path.parent().unwrap().parent().unwrap();
    let java_bin = java_home.join("bin");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", java_bin.to_str().unwrap_or(""), current_path);

    let runner = CommandRunner::new(project_root.to_path_buf())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""))
        .env("JAVA_HOME", java_home.to_str().unwrap_or(""))
        .env("PATH", &new_path);

    let project = load_project()?;
    let product_name = project
        .products
        .first()
        .map(|s| s.as_str())
        .unwrap_or("default");
    let product_arg = format!("product={}", product_name);

    let command_args = [
        "--sync",
        "-p",
        &product_arg,
        "--analyze=normal",
        "--parallel",
        "--incremental",
        "--no-daemon",
    ];

    let program_args: Vec<&str> = std::iter::once(hvigorw_js_path.to_str().unwrap())
        .chain(command_args.iter().copied())
        .collect();

    let node_path_str = node_path.to_str().unwrap_or("node");

    if quiet {
        let output = runner.run_captured_merged(node_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: hvigor sync failed".red());
        }
        Ok(())
    } else {
        run_command_with_log_handling(&runner, node_path_str, &program_args, width, bar)
    }
}

pub fn build(
    args: &BuildArgs,
    project_root: &PathBuf,
    config: &Config,
    width: usize,
    bar: Option<&StatusBar>,
) -> anyhow::Result<()> {
    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let hvigorw_js_path = config
        .hvigorw_js_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 hvigorw.js 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let java_path = config.java_path().ok_or_else(|| {
        anyhow::anyhow!("未找到 Java 路径，请确保 JAVA_HOME 环境变量已设置或 Java 在 PATH 中")
    })?;

    let java_home = java_path.parent().unwrap().parent().unwrap();
    let java_bin = java_home.join("bin");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", java_bin.to_str().unwrap_or(""), current_path);

    let command_args = args.to_command_args(project_root)?;
    let runner = CommandRunner::new(project_root.clone())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""))
        .env("JAVA_HOME", java_home.to_str().unwrap_or(""))
        .env("PATH", &new_path);

    let program_args: Vec<&str> = std::iter::once(hvigorw_js_path.to_str().unwrap())
        .chain(command_args.iter().map(|s| s.as_str()))
        .collect();

    let node_path_str = node_path.to_str().unwrap_or("node");

    if args.quiet {
        let output = runner.run_captured_merged(node_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: build failed".red());
        }
        Ok(())
    } else {
        run_command_with_log_handling(&runner, node_path_str, &program_args, width, bar)
    }
}

fn resolve_tasks(
    module_name: &str,
    target_name: &Option<String>,
    project_root: &PathBuf,
) -> anyhow::Result<Vec<String>> {
    let project = load_project()?;

    if project.root != *project_root {
        anyhow::bail!("project root mismatch");
    }

    if !module_name.is_empty() {
        if let Some(m) = project.find_module(module_name) {
            if let Some(target) = target_name {
                project.validate_target(module_name, target)?;
            }
            let task = match m.module_type {
                ModuleType::Har => "assembleHar".to_string(),
                ModuleType::Shared => "assembleHsp".to_string(),
                _ => "assembleHap".to_string(),
            };
            return Ok(vec![task]);
        } else {
            let available: Vec<&str> = project.modules.iter().map(|m| m.name.as_str()).collect();
            let msg = format!(
                "error: module '{}' not found in project\n\nAvailable modules:\n  {}",
                module_name.red(),
                available.join("\n  ")
            );
            anyhow::bail!("{}", msg);
        }
    }

    if !project.modules.is_empty() {
        let mut has_hap = false;
        let mut has_hsp = false;
        let mut has_har = false;

        for m in &project.modules {
            match m.module_type {
                ModuleType::Entry | ModuleType::Feature => has_hap = true,
                ModuleType::Shared => has_hsp = true,
                ModuleType::Har => has_har = true,
                _ => has_hap = true,
            }
        }

        let mut tasks = Vec::new();
        if has_hap {
            tasks.push("assembleHap".to_string());
        }
        if has_hsp {
            tasks.push("assembleHsp".to_string());
        }
        if has_har {
            tasks.push("assembleHar".to_string());
        }

        if !tasks.is_empty() {
            return Ok(tasks);
        }
    }

    Ok(vec!["assembleHap".to_string()])
}

pub fn clean(
    args: &CleanArgs,
    project_root: &Path,
    config: &Config,
    width: usize,
    bar: Option<&StatusBar>,
) -> anyhow::Result<()> {
    let node_path = config
        .node_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 Node 路径"))?;

    let hvigorw_js_path = config
        .hvigorw_js_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 hvigorw.js 路径"))?;

    let sdk_path = config
        .sdk_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 SDK 路径"))?;

    let mut command_args = vec!["clean".to_string(), "--no-daemon".to_string()];
    if let Some(module) = &args.module {
        command_args.push("-p".to_string());
        command_args.push(format!("module={}", module));
    }

    let runner = CommandRunner::new(project_root.to_path_buf())
        .env("DEVECO_SDK_HOME", sdk_path.to_str().unwrap_or(""));

    let program_args: Vec<&str> = std::iter::once(hvigorw_js_path.to_str().unwrap())
        .chain(command_args.iter().map(|s| s.as_str()))
        .collect();

    let node_path_str = node_path.to_str().unwrap_or("node");

    if args.quiet {
        let output = runner.run_captured_merged(node_path_str, &program_args)?;
        if !output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!("{}", "error: clean failed".red());
        }
        Ok(())
    } else {
        run_command_with_log_handling(&runner, node_path_str, &program_args, width, bar)
    }
}
