use crate::adapters::hvigor;
use crate::config::Config;
use crate::progress::StatusBar;
use crate::project::find_project_root;
use anstream::eprintln;
use clap::Parser;
use clap_complete::engine::ArgValueCompleter;
use owo_colors::OwoColorize;
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
    /// Quiet mode, reduce output
    #[arg(long, short)]
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

pub(crate) fn handle_build(args: BuildArgs) {
    let project_root = match find_project_root() {
        Some(path) => path,
        None => {
            eprintln!(
                "{}",
                "error: no project root found (build-profile.json5)".red()
            );
            std::process::exit(1);
        }
    };

    let config = match Config::load(Some(&project_root)) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", format!("error: failed to load config: {}", e).red());
            std::process::exit(1);
        }
    };

    let project = match crate::project::load_project() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", format!("error: failed to load project: {}", e).red());
            std::process::exit(1);
        }
    };

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
        let entry_modules: Vec<_> = project
            .modules
            .iter()
            .filter(|m| m.module_type == crate::project::ModuleType::Entry)
            .collect();

        let target_module = if entry_modules.len() == 1 {
            Some(entry_modules[0].name.clone())
        } else if project.modules.len() == 1 {
            Some(project.modules[0].name.clone())
        } else {
            None
        };

        if let Some(module_name) = target_module {
            BuildArgs {
                modules: Some(vec![module_name]),
                debug: args.debug,
                release: args.release,
                quiet: args.quiet,
                products: args.products.clone(),
            }
        } else {
            eprintln!(
                "{}",
                "error: no modules specified. Please specify modules using --modules or --products. \
                 (e.g., `heco build --modules entry` or `heco build --products`)".red()
            );
            std::process::exit(1);
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

    let bar = StatusBar::new(total_tasks, args.quiet);
    let total_start = Instant::now();
    let build_type = if args.release { "release" } else { "debug" };

    // 任务 1: Sync
    {
        let _task = bar.task("Syncing", "project");
        if let Err(e) = hvigor::sync(&project_root, &config, args.quiet, 12, Some(&bar)) {
            eprintln!("{}", format!("error: sync failed: {}", e).red());
            std::process::exit(1);
        }
    }

    // 任务 2: Ohpm Install
    {
        let _task = bar.task("Installing", "dependencies");
        if let Err(e) =
            crate::adapters::ohpm::install(&project_root, &config, args.quiet, Some(&bar))
        {
            eprintln!("{}", format!("error: install failed: {}", e).red());
            std::process::exit(1);
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
                format!("{} for product {} ({})", display_name, products[0], project_root.display())
            } else {
                format!("{} ({})", display_name, project_root.display())
            }
        } else {
            format!("{} ({})", display_name, project_root.display())
        };

        let _task = bar.task("Compiling", &desc);

        match hvigor::build(&args, &project_root, &config, 12, Some(&bar)) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{}", format!("error: build failed: {}", e).red());
                std::process::exit(1);
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
            eprintln!("{}", "error: no products found to build".red());
            std::process::exit(1);
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

            match hvigor::build(&single_product_args, &project_root, &config, 12, Some(&bar)) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("error: build failed for product {}: {}", product, e).red()
                    );
                    std::process::exit(1);
                }
            }
        }
    } else {
        // No modules or products specified (should not happen due to earlier validation)
        eprintln!("{}", "error: no modules or products specified".red());
        std::process::exit(1);
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
}
