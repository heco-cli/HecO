use clap::{ColorChoice, Parser, builder::Styles};

mod adapters;
mod build;
mod clean;
mod command;
mod completion;
mod config;
mod device;
mod emulator;
mod env;
mod lint;
mod progress;
mod project;
mod run;

use crate::build::{BuildArgs, handle_build};
use clean::{CleanArgs, handle_clean};
use completion::{CompletionArgs, handle_completion};
use device::{DeviceArgs, handle_device};
use emulator::{EmulatorArgs, handle_emulator};
use env::{EnvArgs, handle_env};
use lint::{LintArgs, handle_lint};
use run::{RunArgs, handle_run};

fn custom_styles() -> Styles {
    use clap::builder::styling::AnsiColor;
    Styles::styled()
        .header(AnsiColor::Yellow.on_default())
        .usage(AnsiColor::Green.on_default())
        .literal(AnsiColor::Cyan.on_default())
        .placeholder(AnsiColor::Blue.on_default())
        .error(AnsiColor::Red.on_default())
        .valid(AnsiColor::BrightCyan.on_default())
        .invalid(AnsiColor::BrightRed.on_default())
}

const BANNER: &str = "\
\x1b[34;1m    __  __\x1b[0m\x1b[36;1m          \x1b[0m\x1b[34;1m____ \x1b[0m\n\
\x1b[34;1m   / / / /\x1b[0m\x1b[36;1m__  _____\x1b[0m\x1b[34;1m/ __ \\\x1b[0m\n\
\x1b[34;1m  / /_/ /\x1b[0m\x1b[36;1m _ \\/ ___/\x1b[0m\x1b[34;1m / / /\x1b[0m\n\
\x1b[34;1m / __  / \x1b[0m\x1b[36;1m __/ /__/\x1b[0m\x1b[34;1m /_/ / \x1b[0m\n\
\x1b[34;1m/_/ /_/\x1b[0m\x1b[36;1m\\___/\\___/\x1b[0m\x1b[34;1m\\____/  \x1b[0m";

#[derive(Parser, Debug)]
#[command(
    name = "HecO", 
    bin_name = "heco",
    before_help = BANNER,
    about = "The HarmonyOS app development CLI tool built for you and AI agents.",
    version,
    author,
    color = ColorChoice::Auto,
    styles = custom_styles(),
)]
struct Cli {
    /// Controls when to use color.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Build modules(s) and product(s)
    #[command(name = "build", display_order = 2)]
    Build(BuildArgs),
    /// Clean build artifacts and uninstall application from devices
    #[command(name = "clean", display_order = 3)]
    Clean(CleanArgs),
    /// Manage environment configurations (e.g., DevEco Studio paths)
    #[command(name = "env", display_order = 1)]
    Env(EnvArgs),
    /// Run code linter (codelinter) and fix issues
    #[command(name = "lint", display_order = 4)]
    Lint(LintArgs),
    /// Manage emulator instances
    #[command(name = "emulator", display_order = 5)]
    Emulator(EmulatorArgs),
    /// Run application on a device or emulator
    #[command(name = "run", display_order = 6)]
    Run(RunArgs),
    /// Manage device(s), include emulator and physical device
    #[command(name = "device", display_order = 7)]
    Device(DeviceArgs),
    /// Generate shell completion scripts
    #[command(name = "completion", display_order = 8)]
    Completion(CompletionArgs),
}

fn main() -> anyhow::Result<()> {
    use clap::CommandFactory;
    use clap_complete::env::CompleteEnv;
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    match cli.color {
        ColorChoice::Always => {
            unsafe { std::env::set_var("CLICOLOR_FORCE", "1") };
            console::set_colors_enabled(true);
            console::set_colors_enabled_stderr(true);
        }
        ColorChoice::Never => {
            unsafe { std::env::set_var("NO_COLOR", "1") };
            console::set_colors_enabled(false);
            console::set_colors_enabled_stderr(false);
        }
        ColorChoice::Auto => {}
    }

    match cli.command {
        Commands::Build(args) => {
            handle_build(args);
        }
        Commands::Clean(args) => {
            handle_clean(args);
        }
        Commands::Env(args) => {
            handle_env(args);
        }
        Commands::Lint(args) => {
            handle_lint(args)?;
        }
        Commands::Emulator(args) => {
            handle_emulator(args)?;
        }
        Commands::Run(args) => {
            handle_run(args)?;
        }
        Commands::Device(args) => {
            handle_device(args)?;
        }
        Commands::Completion(args) => {
            handle_completion(args)?;
        }
    }
    Ok(())
}
