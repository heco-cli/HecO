use crate::adapters::output_parser::{LogType, parse_log_type};
use crate::build::BuildArgs;
use crate::clean::CleanArgs;
use crate::command::{CommandOutputEvent, CommandRunner};
use crate::config::Config;
use crate::output;
use crate::progress::StatusBar;
use crate::project::{ModuleType, load_project};
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};

const HVIGOR_LOG_PREFIXES: &[(LogType, &[&str])] = &[
    (
        LogType::Warning,
        &[
            "WARN: WARN: ArkTS:WARN File:",
            "WARN: ArkTS:WARN File:",
            "WARN: ArkTS:WARN",
            "ArkTS:WARN File:",
            "Warning:",
            "WARN:",
            "ArkTS:WARN",
        ],
    ),
    (LogType::Error, &["ERROR: ArkTS:ERROR", "ERROR:"]),
];

const HVIGOR_SKIPPED_PREFIXES: &[&str] = &["BUILD SUCCESSFUL"];
const HVIGOR_TRIMMED_PREFIXES: &[&str] = &["Finished ", "UP-TO-DATE "];
const HVIGOR_WARNING_TRIMMED_PREFIXES: &[&str] = &["Warning:"];

fn run_command_with_log_handling(
    runner: &CommandRunner,
    node_path_str: &str,
    program_args: &[&str],
    verb: &str,
    bar: Option<&StatusBar>,
) -> anyhow::Result<()> {
    let mut last_log_type: Option<LogType> = None;

    runner.run_with_events(node_path_str, program_args, |event| {
        let raw_line = match event {
            CommandOutputEvent::Line(line) | CommandOutputEvent::Overwrite(line) => line,
        };

        if crate::output::verbose_level() >= 2 {
            let cleaned = anstream::adapter::strip_str(&raw_line)
                .to_string()
                .replace(['\r', '\n'], "");
            let trimmed = cleaned.trim();
            if !trimmed.is_empty() {
                crate::output::line(trimmed);
            }
            return;
        }

        if bar.is_none() {
            return;
        }

        let line = raw_line;

        let mut processed_line = anstream::adapter::strip_str(&line).to_string();
        if processed_line.trim().is_empty() {
            return;
        }

        let is_block_header = processed_line.trim().starts_with("> hvigor ");
        if is_block_header {
            processed_line = processed_line
                .trim_start_matches("> hvigor ")
                .trim_start()
                .to_string();
            last_log_type = None;
        }

        if should_skip_line(&processed_line) {
            return;
        }

        if let Some((log_type, content)) =
            parse_log_type(processed_line.trim(), HVIGOR_LOG_PREFIXES)
        {
            last_log_type = Some(log_type);
            let output = match log_type {
                LogType::Warning => format_warning(&content),
                LogType::Error => format_error(&content),
            };
            emit_plain_line(bar, &output);
        } else if should_continue_log_block(&processed_line, last_log_type) {
            emit_plain_line(bar, processed_line.trim());
        } else {
            last_log_type = None;
            let processed_line = normalize_content_line(&processed_line);
            emit_status_line(bar, verb, &processed_line);
        }
    })?;

    Ok(())
}

fn emit_plain_line(bar: Option<&StatusBar>, content: &str) {
    if let Some(bar) = bar {
        bar.println(content);
    } else {
        output::line(content);
    }
}

fn emit_status_line(bar: Option<&StatusBar>, verb: &str, content: &str) {
    if let Some(bar) = bar {
        bar.status(verb, content);
    } else {
        output::status(verb, content);
    }
}

fn should_skip_line(content: &str) -> bool {
    starts_with_any(content, HVIGOR_SKIPPED_PREFIXES)
}

fn should_continue_log_block(content: &str, last_log_type: Option<LogType>) -> bool {
    if last_log_type.is_none() {
        return false;
    }

    let trimmed = content.trim();
    !trimmed.is_empty() && !looks_like_hvigor_status_line(trimmed)
}

fn normalize_content_line(content: &str) -> String {
    trim_known_prefix_from_set(content, HVIGOR_TRIMMED_PREFIXES)
        .unwrap_or(content)
        .trim()
        .to_string()
}

fn normalize_warning_content(content: &str) -> String {
    trim_known_prefix_from_set(content.trim(), HVIGOR_WARNING_TRIMMED_PREFIXES)
        .unwrap_or(content)
        .trim()
        .to_string()
}

fn trim_known_prefix_from_set<'a>(content: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes
        .iter()
        .find_map(|prefix| content.strip_prefix(prefix))
}

fn starts_with_any(content: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| content.starts_with(prefix))
}

fn looks_like_hvigor_status_line(content: &str) -> bool {
    starts_with_any(content, HVIGOR_TRIMMED_PREFIXES) || content.starts_with(':')
}

fn format_warning(content: &str) -> String {
    format!(
        "{}: {}",
        "warning".yellow().bold(),
        normalize_warning_content(content)
    )
}

fn format_error(content: &str) -> String {
    format!("{}: {}", "error".red().bold(), content)
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

    run_command_with_log_handling(
        &runner,
        node_path_str,
        &program_args,
        "Syncing",
        if quiet { None } else { bar },
    )
}

pub fn build(
    args: &BuildArgs,
    project_root: &PathBuf,
    config: &Config,
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

    run_command_with_log_handling(
        &runner,
        node_path_str,
        &program_args,
        "Compiling",
        if args.quiet { None } else { bar },
    )
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

    run_command_with_log_handling(
        &runner,
        node_path_str,
        &program_args,
        "Cleaning",
        if args.quiet { None } else { bar },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        format_warning, looks_like_hvigor_status_line, normalize_content_line,
        normalize_warning_content, should_continue_log_block, should_skip_line,
    };
    use crate::adapters::output_parser::LogType;

    #[test]
    fn skips_success_summary_lines() {
        assert!(should_skip_line("BUILD SUCCESSFUL in 889 ms"));
        assert!(!should_skip_line("BUILD FAILED in 889 ms"));
    }

    #[test]
    fn trims_finished_prefix() {
        assert_eq!(
            normalize_content_line("Finished :entry:default@BuildJS... after 1 ms"),
            ":entry:default@BuildJS... after 1 ms"
        );
    }

    #[test]
    fn trims_up_to_date_prefix() {
        assert_eq!(
            normalize_content_line("UP-TO-DATE :entry:default@PreBuild..."),
            ":entry:default@PreBuild..."
        );
    }

    #[test]
    fn continues_warning_detail_without_indent() {
        assert!(should_continue_log_block(
            "at /tmp/sample/resources/base/element/float.json",
            Some(LogType::Warning)
        ));
        assert!(should_continue_log_block(
            "but declared again.",
            Some(LogType::Warning)
        ));
    }

    #[test]
    fn continues_unless_line_is_known_status() {
        // Caller is responsible for filtering log-type-prefixed lines before this function.
        assert!(should_continue_log_block(
            "Warning: 'card_content_height' conflict, first declared.",
            Some(LogType::Warning)
        ));
    }

    #[test]
    fn does_not_continue_known_hvigor_status_lines() {
        assert!(looks_like_hvigor_status_line(
            "Finished :phone:default@CompileResource..."
        ));
        assert!(looks_like_hvigor_status_line(
            ":phone:default@CompileArkTS... after 12 s"
        ));
        assert!(!should_continue_log_block(
            "Finished :phone:default@CompileResource... after 468 ms",
            Some(LogType::Warning)
        ));
    }

    #[test]
    fn strips_redundant_warning_prefix_from_content() {
        assert_eq!(
            normalize_warning_content("Warning: 'card_content_height' conflict, first declared."),
            "'card_content_height' conflict, first declared."
        );
        assert_eq!(
            anstream::adapter::strip_str(&format_warning(
                "Warning: 'card_content_height' conflict, first declared."
            ))
            .to_string(),
            "warning: 'card_content_height' conflict, first declared."
        );
    }
}
