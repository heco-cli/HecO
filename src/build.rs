use crate::adapters::hvigor;
use crate::config::Config;
use crate::output;
use crate::progress::StatusBar;
use crate::project::find_project_root;
use clap::Parser;
use clap_complete::engine::ArgValueCompleter;
use owo_colors::OwoColorize;
use std::path::Path;
use std::time::Instant;

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Module names (format: module or module@target), separated by commas. If passed without values, builds all modules.
    #[arg(short, long, num_args = 0.., value_delimiter = ',', add = ArgValueCompleter::new(crate::completion::complete_modules))]
    pub modules: Option<Vec<String>>,
    /// Debug build mode
    #[arg(long, conflicts_with = "release")]
    pub debug: bool,
    /// Release build mode
    #[arg(long, conflicts_with = "debug")]
    pub release: bool,
    #[arg(skip)]
    pub quiet: bool,
    /// Build .app product packages, or specify the product to use when building modules. Separated by commas. If passed without values, builds all products.
    #[arg(long, num_args = 0.., value_delimiter = ',', add = ArgValueCompleter::new(crate::completion::complete_products))]
    pub products: Option<Vec<String>>,
}

impl BuildArgs {
    pub fn parse_modules(&self) -> Option<Vec<(String, Option<String>)>> {
        self.modules.as_ref().map(|modules| {
            modules
                .iter()
                .map(|m| {
                    if let Some(idx) = m.find('@') {
                        let module_name = m[..idx].to_string();
                        let target_name = m[idx + 1..].to_string();
                        (module_name, Some(target_name))
                    } else {
                        (m.clone(), None)
                    }
                })
                .collect()
        })
    }
}

enum AutoBuildModule {
    ByPath(String),
    SingleCandidate(String),
}

fn detect_default_build_module(
    project: &crate::project::Project,
    current_dir: Option<&Path>,
) -> Option<AutoBuildModule> {
    if let Some(current_dir) = current_dir
        && let Some(module) = project.find_module_by_path(current_dir)
    {
        return Some(AutoBuildModule::ByPath(module.name.clone()));
    }

    let entry_modules: Vec<_> = project
        .modules
        .iter()
        .filter(|m| m.module_type == crate::project::ModuleType::Entry)
        .collect();

    if entry_modules.len() == 1 {
        Some(AutoBuildModule::SingleCandidate(
            entry_modules[0].name.clone(),
        ))
    } else if project.modules.len() == 1 {
        Some(AutoBuildModule::SingleCandidate(
            project.modules[0].name.clone(),
        ))
    } else {
        None
    }
}

pub(crate) fn handle_build(args: BuildArgs) -> anyhow::Result<()> {
    let project_root = match find_project_root() {
        Some(path) => path,
        None => {
            anyhow::bail!("no project root found (build-profile.json5)");
        }
    };

    let config = Config::load(Some(&project_root))
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let project = crate::project::load_project()
        .map_err(|e| anyhow::anyhow!("failed to load project: {e}"))?;

    // 先预处理 args，确定有多少个 build 任务
    let args = if let Some(modules) = &args.modules {
        if modules.is_empty() {
            // 如果传入了 --modules 但没有值，收集所有模块名
            let all_modules: Vec<String> = project.modules.iter().map(|m| m.name.clone()).collect();
            BuildArgs {
                modules: Some(all_modules),
                debug: args.debug,
                release: args.release,
                quiet: args.quiet,
                products: args.products.clone(),
            }
        } else {
            args
        }
    } else if args.products.is_none() {
        let current_dir = std::env::current_dir().ok();

        if let Some(selection) = detect_default_build_module(&project, current_dir.as_deref()) {
            let module_name = match selection {
                AutoBuildModule::ByPath(module_name) => {
                    output::status("Detected", format!("module {}", module_name));
                    module_name
                }
                AutoBuildModule::SingleCandidate(module_name) => {
                    output::status("Selected", format!("module {}", module_name));
                    module_name
                }
            };

            BuildArgs {
                modules: Some(vec![module_name]),
                debug: args.debug,
                release: args.release,
                quiet: args.quiet,
                products: args.products.clone(),
            }
        } else {
            anyhow::bail!(
                "no modules specified. Please specify modules using --modules or --products. \
                 (e.g., `heco build --modules entry` or `heco build --products`)"
            );
        }
    } else {
        args
    };

    // 计算总任务数
    let num_build_tasks = if let Some(products) = &args.products {
        if products.is_empty() {
            project.products.len()
        } else {
            products.len()
        }
    } else {
        1
    };
    let total_tasks = 2 + num_build_tasks; // sync + ohpm + build(s)

    let bar = StatusBar::new("Building", total_tasks);
    let total_start = Instant::now();
    let build_type = if args.release { "release" } else { "debug" };

    // 任务 1: Sync
    {
        let _task = bar.task("Syncing", "project");
        if let Err(e) = hvigor::sync(&project_root, &config, args.quiet, Some(&bar)) {
            anyhow::bail!("sync failed: {e}");
        }
    }

    // 任务 2: Ohpm Install
    {
        let _task = bar.task("Installing", "dependencies");
        if let Err(e) =
            crate::adapters::ohpm::install(&project_root, &config, args.quiet, Some(&bar))
        {
            anyhow::bail!("install failed: {e}");
        }
    }

    let parsed_modules = args.parse_modules().unwrap_or_default();

    if args.modules.is_some() {
        // When modules are specified, build them directly (product parameter will be handled in hvigor.rs)
        let display_name = if parsed_modules.is_empty() {
            "project".to_string()
        } else {
            parsed_modules
                .iter()
                .map(|(m, t)| {
                    if let Some(target) = t {
                        format!("{}@{}", m, target)
                    } else {
                        m.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        };

        let desc = if let Some(products) = &args.products {
            if !products.is_empty() {
                format!(
                    "{} for product {} ({})",
                    display_name,
                    products[0],
                    project_root.display()
                )
            } else {
                format!("{} ({})", display_name, project_root.display())
            }
        } else {
            format!("{} ({})", display_name, project_root.display())
        };

        let _task = bar.task("Compiling", &desc);

        match hvigor::build(&args, &project_root, &config, Some(&bar)) {
            Ok(_) => {}
            Err(e) => {
                anyhow::bail!("build failed: {e}");
            }
        }
    } else if let Some(products) = &args.products {
        // Only products specified, loop through them
        let target_products = if products.is_empty() {
            project.products.clone()
        } else {
            products.clone()
        };

        if target_products.is_empty() {
            anyhow::bail!("no products found to build");
        }

        for product in &target_products {
            let desc = format!("product {} ({})", product, project_root.display());
            let _task = bar.task("Compiling", &desc);

            // Create a temporary args just for this product
            let single_product_args = BuildArgs {
                modules: args.modules.clone(),
                debug: args.debug,
                release: args.release,
                quiet: args.quiet,
                products: Some(vec![product.clone()]),
            };

            match hvigor::build(&single_product_args, &project_root, &config, Some(&bar)) {
                Ok(_) => {}
                Err(e) => {
                    anyhow::bail!("build failed for product {product}: {e}");
                }
            }
        }
    } else {
        anyhow::bail!("no modules or products specified");
    }

    // 结束，显示总完成信息
    if !args.quiet {
        bar.finish_with_message(&format!(
            "{:>12} {} in {:.2?}",
            "Finished".green().bold(),
            build_type,
            total_start.elapsed()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AutoBuildModule, detect_default_build_module};
    use crate::project::{Module, ModuleType, Project};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_module_from_nested_subdirectory() {
        let root = create_test_project(
            "build-detect-subdir",
            &[("entry", "./entry"), ("feature", "./feature")],
        );
        let project = test_project(
            &root,
            vec![
                test_module("entry", ModuleType::Entry, "./entry"),
                test_module("feature", ModuleType::Feature, "./feature"),
            ],
        );
        let nested_dir = root.join("feature").join("src").join("main");

        let selection = detect_default_build_module(&project, Some(&nested_dir));

        assert!(matches!(
            selection,
            Some(AutoBuildModule::ByPath(module)) if module == "feature"
        ));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn falls_back_to_single_entry_module() {
        let root = create_test_project(
            "build-detect-single-entry",
            &[("entry", "./entry"), ("library", "./library")],
        );
        let project = test_project(
            &root,
            vec![
                test_module("entry", ModuleType::Entry, "./entry"),
                test_module("library", ModuleType::Har, "./library"),
            ],
        );

        let selection = detect_default_build_module(&project, Some(&root));

        assert!(matches!(
            selection,
            Some(AutoBuildModule::SingleCandidate(module)) if module == "entry"
        ));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn returns_none_when_multiple_modules_are_possible() {
        let root = create_test_project(
            "build-detect-ambiguous",
            &[("entry", "./entry"), ("demo", "./demo")],
        );
        let project = test_project(
            &root,
            vec![
                test_module("entry", ModuleType::Entry, "./entry"),
                test_module("demo", ModuleType::Entry, "./demo"),
            ],
        );

        let selection = detect_default_build_module(&project, Some(&root));

        assert!(selection.is_none());

        fs::remove_dir_all(&root).unwrap();
    }

    fn test_project(root: &Path, modules: Vec<Module>) -> Project {
        Project {
            root: root.to_path_buf(),
            modules,
            products: vec!["default".to_string()],
        }
    }

    fn test_module(name: &str, module_type: ModuleType, src_path: &str) -> Module {
        Module {
            name: name.to_string(),
            module_type,
            targets: vec!["default".to_string()],
            src_path: src_path.to_string(),
        }
    }

    fn create_test_project(name: &str, modules: &[(&str, &str)]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("heco-{name}-{unique}"));

        fs::create_dir_all(&root).unwrap();

        let module_entries = modules
            .iter()
            .map(|(module_name, src_path)| {
                format!(r#"{{ name: "{module_name}", srcPath: "{src_path}" }}"#)
            })
            .collect::<Vec<_>>()
            .join(",\n      ");

        fs::write(
            root.join("build-profile.json5"),
            format!(
                r#"{{
  app: {{
    products: [{{ name: "default" }}]
  }},
  modules: [
      {module_entries}
  ]
}}"#
            ),
        )
        .unwrap();

        for (_, src_path) in modules {
            let relative = src_path.strip_prefix("./").unwrap_or(src_path);
            fs::create_dir_all(root.join(relative).join("src").join("main")).unwrap();
        }

        root
    }
}
