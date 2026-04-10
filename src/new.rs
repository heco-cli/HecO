use clap::Args;
use dialoguer::Input;
use std::path::Path;

#[derive(Args, Debug)]
pub struct NewArgs {
    /// Path to create the HarmonyOS project
    pub path: String,
    /// Custom project name, defaults to the last part of the project path
    #[arg(short, long)]
    pub name: Option<String>,
    #[arg(long)]
    pub bundle_name: Option<String>,
    #[arg(long)]
    pub api_level: Option<u32>,
}

pub(crate) fn get_project_name(path: &str, custom_name: &Option<String>) -> String {
    if let Some(name) = custom_name {
        return name.clone();
    }

    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

pub(crate) fn handle_new(args: NewArgs) {
    let project_name = get_project_name(&args.path, &args.name);

    let bundle_name = if let Some(name) = args.bundle_name {
        name
    } else {
        get_bundle_name()
    };

    let api_level = if let Some(level) = args.api_level {
        level
    } else {
        get_api_level()
    };

    println!("Creating new HarmonyOS project:");
    println!("  Project path: {}", args.path);
    println!("  Project name: {}", project_name);
    println!("  Bundle name: {}", bundle_name);
    println!("  API level: {}", api_level);
}

pub fn get_bundle_name() -> String {
    Input::new()
        .with_prompt("Please enter bundle name (e.g., org.example.org)")
        .default("com.example.org".to_string())
        .interact()
        .unwrap()
}

pub fn get_api_level() -> u32 {
    Input::new()
        .with_prompt("Please enter API level (e.g., 22-28)")
        .default("22".to_string())
        .interact()
        .unwrap()
        .parse::<u32>()
        .unwrap_or(22)
}
