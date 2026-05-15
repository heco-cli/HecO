use crate::adapters::output_parser::{LogType, parse_log_type};
use crate::command::{CommandOutputEvent, CommandRunner};
use crate::config::Config;
use crate::output;
use crate::progress::StatusBar;
use crate::project::{find_project_root, load_project};
use anyhow::{Context, Result};
use clap::Parser;
use clap_complete::engine::ArgValueCompleter;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "lint")]
pub struct LintArgs {
    /// Automatically fix fixable issues
    #[arg(long)]
    pub fix: bool,
    /// Specify one or more product names, separated by commas
    #[arg(long, value_delimiter = ',', add = ArgValueCompleter::new(crate::completion::complete_products))]
    pub products: Option<Vec<String>>,
    #[arg(skip)]
    pub quiet: bool,
}

const LINT_LOG_PREFIXES: &[(LogType, &[&str])] = &[
    (LogType::Warning, &["warning:", "Warning:", "WARN:"]),
    (LogType::Error, &["error:", "Error:", "ERROR:"]),
];

fn run_codelinter(
    project_root: &std::path::Path,
    config: &Config,
    check_path: &str,
    fix: bool,
    product: Option<&str>,
    quiet: bool,
    bar: Option<&StatusBar>,
) -> Result<()> {
    let node_path = config.node_path().context(
        "Node runtime not found. Please check DevEco Studio installation path configuration.",
    )?;

    let codelinter_path = config.codelinter_path().context(
        "codelinter tool not found. Please check DevEco Studio installation path configuration.",
    )?;

    if !codelinter_path.exists() {
        return Err(anyhow::anyhow!(
            "codelinter tool not found at path: {}. Please check DevEco Studio installation path configuration.",
            codelinter_path.display()
        ));
    }

    let codelinter_str = codelinter_path.to_string_lossy().to_string();

    let mut cmd_args: Vec<String> = vec![codelinter_str];

    if fix {
        cmd_args.push("--fix".to_string());
    }

    if let Some(product_name) = product {
        cmd_args.push("-p".to_string());
        cmd_args.push(product_name.to_string());
    }

    if !check_path.is_empty() && check_path != "." {
        cmd_args.push(check_path.to_string());
    }

    let cmd_args_ref: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
    let node_path_str = node_path.to_str().unwrap_or("node");
    let runner = CommandRunner::new(project_root.to_path_buf());

    let mut last_log_type: Option<LogType> = None;

    runner.run_with_events(node_path_str, &cmd_args_ref, |event| {
        let raw_line = match event {
            CommandOutputEvent::Line(line) | CommandOutputEvent::Overwrite(line) => line,
        };

        if crate::output::verbose_level() >= 2 {
            let cleaned = sanitize_lint_line(&raw_line);
            if !cleaned.is_empty() {
                crate::output::line(&cleaned);
            }
            return;
        }

        if quiet {
            return;
        }

        let cleaned_line = sanitize_lint_line(&raw_line);
        if cleaned_line.is_empty() {
            return;
        }

        if let Some((percent, message)) = parse_lint_progress(&cleaned_line) {
            if !quiet && let Some(bar) = bar {
                bar.set_progress(percent, &message);
            }
            return;
        }

        if let Some((verb, description)) = parse_lint_status_line(&cleaned_line) {
            emit_status_line(bar, verb, &description);
            return;
        }

        if is_lint_info_line(&cleaned_line) {
            return;
        }

        if let Some((log_type, content)) = parse_log_type(&cleaned_line, LINT_LOG_PREFIXES) {
            last_log_type = Some(log_type);
            let formatted = match log_type {
                LogType::Warning => format!("warning: {}", content),
                LogType::Error => format!("error: {}", content),
            };
            emit_result_line(&formatted);
            return;
        }

        if raw_line.starts_with(char::is_whitespace) && last_log_type.is_some() {
            emit_result_line(&cleaned_line);
        } else {
            last_log_type = None;
            emit_result_line(&cleaned_line);
        }
    })
}

fn sanitize_lint_line(line: &str) -> String {
    anstream::adapter::strip_str(line)
        .to_string()
        .replace(['\r', '\n'], "")
        .trim()
        .to_string()
}

fn parse_lint_progress(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim();
    let start = trimmed.find('[')?;
    let bar_end = trimmed[start + 1..].find(']')? + start + 1;
    let percent_text = trimmed[bar_end + 1..].trim();
    let percent = percent_text
        .strip_suffix('%')?
        .trim()
        .parse::<usize>()
        .ok()?;

    let prefix = trimmed[..start].trim();
    if !prefix.starts_with("Working...")
        && !prefix.starts_with("Finished...")
        && !prefix.starts_with("Checking...")
    {
        return None;
    }

    let message = if prefix.starts_with("Finished...") {
        "lint finished".to_string()
    } else {
        "running lint checks".to_string()
    };

    Some((percent.min(100), message))
}

fn is_lint_info_line(line: &str) -> bool {
    line == "No defects found in your code."
}

fn parse_lint_status_line(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim();

    if let Some(product) = trimmed.strip_prefix("Currently active product:") {
        let product = product.trim();
        if !product.is_empty() {
            return Some(("Using", format!("product {}", product)));
        }
    }

    if let Some(path) = parse_lint_config_path(trimmed) {
        return Some(("Using", format!("config {}", path)));
    }

    None
}

fn parse_lint_config_path(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("The configuration file ")?;
    let path = rest.strip_suffix(" in the project is in use.")?;
    Some(path.trim())
}

fn emit_result_line(content: &str) {
    output::line(content);
}

fn emit_status_line(bar: Option<&StatusBar>, verb: &str, description: &str) {
    if let Some(bar) = bar {
        bar.status(verb, description);
    } else {
        output::status(verb, description);
    }
}

pub fn handle_lint(args: LintArgs) -> Result<()> {
    let project_root = find_project_root()
        .context("no HMOS project root found (missing build-profile.json5 or oh-package.json5)")?;

    let project = load_project().context("failed to load project info")?;

    let config = Config::load(Some(&project_root))?;

    let current_dir = std::env::current_dir()?;

    let relative_path = current_dir
        .strip_prefix(&project_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let check_path = if relative_path.is_empty() || relative_path == "." {
        ".".to_string()
    } else {
        relative_path
    };

    let start = Instant::now();

    if let Some(ref products) = args.products {
        for product in products {
            project.validate_product(product)?;
            if !args.quiet {
                output::status(
                    "Checking",
                    format!("{} ({})", product, project_root.display()),
                );
            }
            let lint_bar = StatusBar::new("Checking", 100);
            run_codelinter(
                &project_root,
                &config,
                &check_path,
                args.fix,
                Some(product),
                args.quiet,
                if args.quiet { None } else { Some(&lint_bar) },
            )?;
            lint_bar.finish_and_clear();
        }

        if !args.quiet {
            output::finished("", start.elapsed());
        }
    } else {
        if !args.quiet {
            output::status("Checking", project_root.display());
        }

        let lint_bar = StatusBar::new("Checking", 100);

        run_codelinter(
            &project_root,
            &config,
            &check_path,
            args.fix,
            None,
            args.quiet,
            if args.quiet { None } else { Some(&lint_bar) },
        )?;

        lint_bar.finish_and_clear();

        if !args.quiet {
            output::finished("", start.elapsed());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_lint_status_line;

    #[test]
    fn parses_active_product_line_as_status() {
        assert_eq!(
            parse_lint_status_line("Currently active product: default"),
            Some(("Using", "product default".to_string()))
        );
    }

    #[test]
    fn parses_config_file_line_as_status() {
        assert_eq!(
            parse_lint_status_line(
                "The configuration file /path/to/code-linter.json5 in the project is in use."
            ),
            Some(("Using", "config /path/to/code-linter.json5".to_string()))
        );
    }

    #[test]
    fn ignores_unrelated_status_lines() {
        assert_eq!(
            parse_lint_status_line("CodeLinter found some defects in your code."),
            None
        );
    }
}
