use clap::Parser;
mod build;
mod check;
mod new;
mod run;
mod test;

use build::{handle_build, BuildArgs};
use check::{handle_check, CheckArgs};
use new::{handle_new, NewArgs};
use run::{handle_run, RunArgs};
use test::{handle_test, TestArgs};

#[derive(Parser, Debug)]
#[command(name = "heco")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    New(NewArgs),
    Build(BuildArgs),
    Run(RunArgs),
    Check(CheckArgs),
    Test(TestArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New(args) => {
            handle_new(args);
        }
        Commands::Build(args) => {
            handle_build(args);
        }
        Commands::Run(args) => {
            handle_run(args);
        }
        Commands::Check(args) => {
            handle_check(args);
        }
        Commands::Test(args) => {
            handle_test(args);
        }
    }

    Ok(())
}
